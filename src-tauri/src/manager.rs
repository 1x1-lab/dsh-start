use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::commands::StatusPayload;
use crate::logger::log_event;
use crate::runtime;
use crate::state::{AppState, DshStatus};

pub const READY_POLL_MS: u64 = 300;
pub const READY_TIMEOUT_SECS: u64 = 30;
pub const STOP_GRACE_SECS: u64 = 5;
pub const MAX_CRASH_RETRIES: u32 = 5;
const MIN_RESTART_INTERVAL: Duration = Duration::from_millis(1500);

fn backoff_secs(n: u32) -> u64 {
    (2u64.saturating_pow(n.saturating_sub(1))).min(30)
}

pub(crate) fn set_status(app: &AppHandle, status: DshStatus) {
    app.state::<AppState>().manager.lock().unwrap().status = status;
    emit_status(app);
}

pub fn status_payload(app: &AppHandle) -> StatusPayload {
    let state = app.state::<AppState>();
    let st = state.manager.lock().unwrap();
    let settings = state.settings.lock().unwrap();
    let node = state.node_info.lock().unwrap().clone();
    let control = state.control_port.lock().unwrap().clone();
    let autostart = {
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().is_enabled().unwrap_or(false)
    };
    StatusPayload {
        status: st.status.as_str().to_string(),
        pid: st.pid,
        port: settings.port,
        installed_version: st
            .installed_version
            .clone()
            .or_else(|| runtime::installed_version(app)),
        uptime_ms: st.started_at.map(|t| t.elapsed().as_millis() as u64),
        last_error: st.last_error.clone(),
        control_port: control,
        autostart,
        node_present: node.is_some(),
        node_version: node.map(|n| n.node_version),
        crash_restart: settings.crash_restart,
    }
}

pub fn emit_status(app: &AppHandle) {
    let payload = status_payload(app);
    let _ = app.emit("dsh-status", &payload);
    crate::tray::update(app);
}

/// Start dsh: ensure node + managed install exist, then spawn the child.
pub fn start(app: &AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let st = state.manager.lock().unwrap();
        if st.status == DshStatus::Starting
            || st.status == DshStatus::Running
            || st.child_alive.load(Ordering::SeqCst)
        {
            return Ok(());
        }
        if st.status == DshStatus::ExternalRunning {
            let port = state.settings.lock().unwrap().port;
            let msg = format!(
                "端口 {port} 上已有外部启动的 DSH 在运行；如需本应用托管，请先停止该实例"
            );
            log_event(app, "warn", &msg);
            return Err(msg);
        }
    }
    let node_info = runtime::detect_node().map_err(|e| {
        set_status(app, DshStatus::NodeMissing);
        e
    })?;
    {
        *app.state::<AppState>().node_info.lock().unwrap() = Some(node_info);
    }

    if runtime::installed_version(app).is_none() {
        log_event(app, "info", "DSH 尚未安装，开始自动安装（npm install）…");
        set_status(app, DshStatus::Installing);
        let version = {
            app.state::<AppState>().settings.lock().unwrap().dsh_version.clone()
        };
        let mut forward = runtime::forward_install_lines(app);
        match runtime::install(app, &version, &mut forward) {
            Ok(v) => {
                log_event(app, "info", &format!("DSH 安装完成：v{v}"));
            }
            Err(e) => {
                set_status(app, DshStatus::Error);
                return Err(e);
            }
        }
    }

    spawn(app)
}

fn spawn(app: &AppHandle) -> Result<(), String> {
    let node_info = runtime::detect_node().map_err(|e| e)?;
    let bin = runtime::dsh_bin(app).ok_or("DSH 未安装，请先完成安装")?;
    let port = app.state::<AppState>().settings.lock().unwrap().port;

    if tcp_probe(port) {
        if http_probe(port) {
            let msg = format!(
                "端口 {port} 上已有外部启动的 DSH 在运行；如需本应用托管，请先停止该实例"
            );
            log_event(app, "warn", &msg);
            set_status(app, DshStatus::ExternalRunning);
            return Err(msg);
        }
        let msg = format!("端口 {port} 已被其他程序占用，请在设置中更换端口");
        log_event(app, "warn", &msg);
        set_status(app, DshStatus::PortInUse);
        return Err(msg);
    }

    let shim_dir = app.state::<AppState>().shim_dir.lock().unwrap().clone();

    let mut cmd = Command::new(&node_info.node);
    cmd.arg(&bin)
        .arg("web")
        .arg("--port")
        .arg(port.to_string())
        .current_dir(runtime::runtime_dir(app))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // make the callback shim resolvable from inside dsh's own shell tools
    if let Some(sd) = &shim_dir {
        if let Some(path) = std::env::var_os("PATH") {
            let mut paths = std::env::split_paths(&path).collect::<Vec<_>>();
            paths.insert(0, sd.clone());
            if let Ok(p) = std::env::join_paths(&paths) {
                cmd.env("PATH", p);
            }
        }
        cmd.env("DSH_START_EXE", std::env::current_exe().unwrap_or_default());
    }
    cmd.env("DSH_START_PORT", port.to_string());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // own process group → group-kill later
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| {
            set_status(app, DshStatus::Error);
            format!("启动 DSH 失败: {e}")
        })?;
    let pid = child.id();
    log_event(app, "info", &format!("DSH 已启动 (pid={pid}, port={port})"));

    {
        let state = app.state::<AppState>();
        let mut st = state.manager.lock().unwrap();
        st.pid = Some(pid);
        st.status = DshStatus::Starting;
        st.started_at = None;
        st.last_error = None;
        st.addr_in_use.store(false, Ordering::SeqCst);
        st.intentional_stop.store(false, Ordering::SeqCst);
        st.child_alive.store(true, Ordering::SeqCst);
    }
    emit_status(app);

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let app_out = app.clone();
    std::thread::spawn(move || pipe_lines(stdout, &app_out, "out"));
    let app_err = app.clone();
    std::thread::spawn(move || pipe_lines(stderr, &app_err, "err"));

    let app_mon = app.clone();
    std::thread::spawn(move || monitor_child(app_mon, child));

    let app_ready = app.clone();
    std::thread::spawn(move || wait_ready(app_ready, port));

    Ok(())
}

fn pipe_lines<R: std::io::Read>(reader: R, app: &AppHandle, tag: &str) {
    use std::io::BufReader;
    for line in BufRead::lines(BufReader::new(reader)) {
        match line {
            Ok(l) => {
                let clean = runtime::strip_ansi(&l);
                if !clean.trim().is_empty() {
                    log_event(app, "info", &format!("[dsh:{tag}] {}", clean.trim()));
                    if clean.contains("EADDRINUSE") || clean.contains("address already in use") {
                        app.state::<AppState>()
                            .manager
                            .lock()
                            .unwrap()
                            .addr_in_use
                            .store(true, Ordering::SeqCst);
                    }
                }
            }
            Err(_) => break,
        }
    }
}

fn monitor_child(app: AppHandle, mut child: Child) {
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(500)),
            Err(e) => {
                log_event(&app, "error", &format!("监控 DSH 失败: {e}"));
                break None;
            }
        }
    };
    let code = exit_status.and_then(|s| s.code());

    let (intentional, addr_in_use, crash_restart, retries_left) = {
        let state = app.state::<AppState>();
        let mut st = state.manager.lock().unwrap();
        st.child_alive.store(false, Ordering::SeqCst);
        st.pid = None;
        let intentional = st.intentional_stop.load(Ordering::SeqCst);
        let addr_in_use = st.addr_in_use.load(Ordering::SeqCst);
        if intentional {
            st.status = DshStatus::Stopped;
            st.intentional_stop.store(false, Ordering::SeqCst);
            (true, false, false, 0)
        } else {
            st.status = DshStatus::Crashed;
            let msg = match code {
                Some(c) => format!("DSH 进程异常退出（exit={c}）"),
                None => "DSH 进程异常退出".to_string(),
            };
            st.last_error = Some(msg.clone());
            log_event(&app, "error", &msg);
            let crash_restart = state.settings.lock().unwrap().crash_restart;
            let retries_left = st.crash_retries_left;
            let retry = crash_restart && retries_left > 0;
            if retry {
                st.crash_retries_left = retries_left - 1;
            }
            (false, addr_in_use, retry, retries_left)
        }
    };

    if intentional {
        emit_status(&app);
        log_event(&app, "info", "DSH 已停止");
        return;
    }
    if addr_in_use {
        set_status(&app, DshStatus::PortInUse);
        log_event(&app, "error", "端口被占用（EADDRINUSE），请更换端口后重试");
        return;
    }
    emit_status(&app);
    if crash_restart {
        let backoff = backoff_secs(MAX_CRASH_RETRIES - retries_left + 1);
        log_event(
            &app,
            "info",
            &format!("{backoff}s 后自动重启 DSH（剩余重试 {retries_left} 次）"),
        );
        std::thread::sleep(Duration::from_secs(backoff));
        let _ = start(&app);
    }
}

fn wait_ready(app: AppHandle, port: u16) {
    let deadline = Instant::now() + Duration::from_secs(READY_TIMEOUT_SECS);
    loop {
        let alive = app
            .state::<AppState>()
            .manager
            .lock()
            .unwrap()
            .child_alive
            .load(Ordering::SeqCst);
        if !alive {
            return; // child exited before ready; monitor handles it
        }
        if http_probe(port) {
            let version = runtime::installed_version(&app);
            {
                let state = app.state::<AppState>();
                let mut st = state.manager.lock().unwrap();
                st.status = DshStatus::Running;
                st.started_at = Some(Instant::now());
                st.crash_retries_left = MAX_CRASH_RETRIES;
                st.last_error = None;
                st.installed_version = version;
            }
            emit_status(&app);
            log_event(&app, "info", &format!("DSH 就绪：http://127.0.0.1:{port}"));
            return;
        }
        if Instant::now() > deadline {
            {
                let state = app.state::<AppState>();
                let mut st = state.manager.lock().unwrap();
                if st.child_alive.load(Ordering::SeqCst) {
                    st.status = DshStatus::Error;
                    st.last_error = Some(format!(
                        "DSH 启动超时（{READY_TIMEOUT_SECS}s 内未就绪）"
                    ));
                }
            }
            emit_status(&app);
            return;
        }
        std::thread::sleep(Duration::from_millis(READY_POLL_MS));
    }
}

/// Stop the dsh child: graceful signal first, force kill after grace period.
pub fn stop(app: &AppHandle) -> Result<(), String> {
    let pid = app.state::<AppState>().manager.lock().unwrap().pid;
    let Some(pid) = pid else {
        return Ok(());
    };
    app.state::<AppState>()
        .manager
        .lock()
        .unwrap()
        .intentional_stop
        .store(true, Ordering::SeqCst);
    log_event(app, "info", &format!("正在停止 DSH (pid={pid})…"));
    signal_terminate(pid);

    let deadline = Instant::now() + Duration::from_secs(STOP_GRACE_SECS);
    while Instant::now() < deadline {
        let alive = app
            .state::<AppState>()
            .manager
            .lock()
            .unwrap()
            .child_alive
            .load(Ordering::SeqCst);
        if !alive {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    log_event(app, "warn", "优雅停止超时，强制结束 DSH 进程");
    force_kill(pid);
    Ok(())
}

/// Unified restart: stop (if any) then start. Soft-throttled against storms.
pub fn restart(app: &AppHandle, reason: &str) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let mut st = state.manager.lock().unwrap();
        let now = Instant::now();
        if let Some(last) = st.last_restart_at {
            if now.duration_since(last) < MIN_RESTART_INTERVAL {
                return Ok(());
            }
        }
        st.last_restart_at = Some(now);
    }
    log_event(app, "info", &format!("收到重启请求（来源：{reason}）"));
    let _ = stop(app);
    start(app)
}

pub fn open_web(app: &AppHandle) {
    let port = app.state::<AppState>().settings.lock().unwrap().port;
    let url = format!("http://127.0.0.1:{port}");
    if let Err(e) = tauri_plugin_opener::open_url(&url, None::<&str>) {
        log_event(app, "error", &format!("打开浏览器失败: {e}"));
    }
}

/// External-instance watcher: while no managed child is alive, probe the
/// configured port so a dsh started outside this app still shows as running
/// (`external`, not managed: no pid/uptime/crash-restart). Reverts to idle
/// when the port stops answering.
pub fn watch_external(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1500));
        let (status, child_alive, port) = {
            let state = app.state::<AppState>();
            let st = state.manager.lock().unwrap();
            let port = state.settings.lock().unwrap().port;
            (st.status, st.child_alive.load(Ordering::SeqCst), port)
        };
        // managed lifecycle in progress → the manager owns the status
        if child_alive
            || matches!(
                status,
                DshStatus::Starting | DshStatus::Installing | DshStatus::NodeMissing
            )
        {
            continue;
        }

        let reachable = http_probe(port);
        match (status, reachable) {
            (DshStatus::ExternalRunning, false) => {
                log_event(&app, "info", "外部 DSH 实例已停止");
                set_status(&app, DshStatus::InstalledIdle);
            }
            (DshStatus::PortInUse, false) => {
                // whatever occupied the port is gone
                set_status(&app, DshStatus::InstalledIdle);
            }
            (DshStatus::ExternalRunning, true) => {}
            (_, true) => {
                log_event(
                    &app,
                    "info",
                    &format!("检测到端口 {port} 上运行中的 DSH（外部实例，未托管）"),
                );
                {
                    let state = app.state::<AppState>();
                    let mut st = state.manager.lock().unwrap();
                    st.status = DshStatus::ExternalRunning;
                    st.pid = None;
                    st.started_at = None;
                    st.last_error = None;
                }
                emit_status(&app);
            }
            (_, false) => {}
        }
    });
}

fn signal_terminate(pid: u32) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(-(pid as i32)), Signal::SIGTERM);
    }
    #[cfg(windows)]
    {
        // 子进程以 CREATE_NO_WINDOW 启动，没有窗口收不到 WM_CLOSE（taskkill 不带
        // /F 的软化方式必然无效），直接强制结束进程树，省去 5s 优雅停止超时
        force_kill(pid);
    }
}

fn force_kill(pid: u32) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL);
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW：避免黑窗口闪现
        }
        let _ = cmd.status();
    }
}

fn tcp_probe(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

fn http_probe(port: u16) -> bool {
    use std::io::{Read, Write};
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(1000),
    ) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1200)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(1200)));
    let req = format!(
        "GET / HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 1024];
    match stream.read(&mut buf) {
        Ok(n) if n > 0 => {
            let head = String::from_utf8_lossy(&buf[..n]);
            head.starts_with("HTTP/1.")
        }
        _ => false,
    }
}
