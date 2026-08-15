use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::manager;
use crate::runtime;
use crate::settings::{self, Settings};
use crate::state::{AppState, DshStatus};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub status: String,
    pub pid: Option<u32>,
    pub port: u16,
    pub installed_version: Option<String>,
    /// 系统层面（PATH 全局安装 / npm npx 缓存）存在的 dsh 版本；None = 未发现
    pub system_dsh_version: Option<String>,
    pub uptime_ms: Option<u64>,
    pub last_error: Option<String>,
    pub control_port: Option<u16>,
    pub autostart: bool,
    pub node_present: bool,
    pub node_version: Option<String>,
    pub crash_restart: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub node_present: bool,
    pub node_version: Option<String>,
    pub installed_version: Option<String>,
    pub system_dsh_version: Option<String>,
    pub runtime_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallResult {
    pub version: String,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub installed: Option<String>,
    pub latest: String,
    pub update_available: bool,
}

/// Query the npm registry for the version the configured spec would install,
/// compared against the managed install.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateCheck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let latest = runtime::latest_version(&app)?;
        let installed = runtime::installed_version(&app);
        Ok(UpdateCheck {
            update_available: installed.as_deref() != Some(latest.as_str()),
            installed,
            latest,
        })
    })
    .await
    .map_err(|e| format!("检查更新任务异常: {e}"))?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallbackInfo {
    pub http_url: String,
    pub http_port: Option<u16>,
    pub cli_cmd: String,
}

#[tauri::command]
pub fn get_status(app: AppHandle) -> StatusPayload {
    manager::status_payload(&app)
}

#[tauri::command]
pub fn start_dsh(app: AppHandle) -> Result<(), String> {
    manager::start(&app)
}

#[tauri::command]
pub fn stop_dsh(app: AppHandle) -> Result<(), String> {
    manager::stop(&app)
}

#[tauri::command]
pub fn restart_dsh(app: AppHandle, reason: Option<String>) -> Result<(), String> {
    manager::restart(&app, &reason.unwrap_or_else(|| "ui".into()))
}

#[tauri::command]
pub fn force_stop_external(app: AppHandle) -> Result<(), String> {
    manager::force_stop_external(&app)
}

/// 升级系统（非托管）安装的 dsh：按它原本的方式原地升级
/// （全局安装 → npm install -g；npx 缓存 → npx 刷新），不转为托管。
#[tauri::command]
pub async fn upgrade_system_dsh(app: AppHandle, version: Option<String>) -> Result<String, String> {
    let v = version.unwrap_or_else(|| "latest".into());
    tauri::async_runtime::spawn_blocking(move || {
        let result = runtime::upgrade_system_dsh(&app, &v);
        match &result {
            Ok(_) => manager::emit_status(&app),
            Err(e) => crate::logger::log_event(&app, "error", &format!("系统 DSH 升级失败：{e}")),
        }
        result
    })
    .await
    .map_err(|e| format!("系统 DSH 升级任务异常: {e}"))?
}

#[tauri::command]
pub fn get_runtime_info(app: AppHandle) -> RuntimeInfo {
    let node = runtime::detect_node().ok();
    RuntimeInfo {
        node_present: node.is_some(),
        node_version: node.as_ref().map(|n| n.node_version.clone()),
        installed_version: runtime::installed_version(&app),
        system_dsh_version: runtime::system_dsh_version(),
        runtime_dir: runtime::runtime_dir(&app).display().to_string(),
    }
}

/// Long-running: install (or update) dsh via npm. Progress is streamed through
/// `install-progress` events.
#[tauri::command]
pub async fn ensure_runtime(app: AppHandle, version: Option<String>) -> Result<RuntimeInstallResult, String> {
    let requested = version.unwrap_or_else(|| "latest".into());
    tauri::async_runtime::spawn_blocking(move || {
        manager::set_status(&app, DshStatus::Installing);
        let prev = runtime::installed_version(&app);
        let result = {
            let mut forward = runtime::forward_install_lines(&app);
            runtime::install(&app, &requested, &mut forward)
        };
        match result {
            Ok(new_v) => {
                crate::logger::log_event(&app, "info", &format!("DSH 安装完成：v{new_v}"));
                let changed = prev.as_deref() != Some(new_v.as_str());
                {
                    let state = app.state::<AppState>();
                    let mut st = state.manager.lock().unwrap();
                    st.installed_version = Some(new_v.clone());
                    st.status = DshStatus::InstalledIdle;
                }
                manager::emit_status(&app);
                Ok(RuntimeInstallResult {
                    version: new_v,
                    changed,
                })
            }
            Err(e) => {
                crate::logger::log_event(&app, "error", &format!("DSH 安装失败：{e}"));
                manager::set_status(&app, DshStatus::Error);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("安装任务异常: {e}"))?
}

/// Guided Node.js install: try winget/brew/apt, fall back to opening the
/// official download page.
#[tauri::command]
pub fn install_node_guided(app: AppHandle) -> Result<String, String> {
    #[cfg(windows)]
    {
        if runtime::find_on_path("winget").is_some() {
            crate::logger::log_event(&app, "info", "通过 winget 安装 Node.js LTS…");
            let mut fwd = |line: &str| {
                let _ = app.emit("install-progress", line.to_string());
            };
            let ok = runtime::run_streaming(
                "winget",
                &[
                    "install",
                    "--id",
                    "OpenJS.NodeJS.LTS",
                    "--accept-package-agreements",
                    "--accept-source-agreements",
                ],
                &mut fwd,
            )
            .map(|s| s.success())
            .unwrap_or(false);
            if ok {
                return Ok("Node.js 已通过 winget 安装完成，重启应用后生效。".into());
            }
        }
        let _ = tauri_plugin_opener::open_url("https://nodejs.org", None::<&str>);
        return Err("自动安装 Node.js 失败，已打开官方下载页，请手动安装后重启应用。".into());
    }
    #[cfg(target_os = "macos")]
    {
        if runtime::find_on_path("brew").is_some() {
            crate::logger::log_event(&app, "info", "通过 Homebrew 安装 Node…");
            let mut fwd = |line: &str| {
                let _ = app.emit("install-progress", line.to_string());
            };
            let ok = runtime::run_streaming("brew", &["install", "node"], &mut fwd)
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Ok("Node.js 已通过 Homebrew 安装完成，重启应用后生效。".into());
            }
        }
        let _ = tauri_plugin_opener::open_url("https://nodejs.org", None::<&str>);
        return Err("自动安装 Node.js 失败，已打开官方下载页，请手动安装后重启应用。".into());
    }
    #[cfg(target_os = "linux")]
    {
        let mut fwd = |line: &str| {
            let _ = app.emit("install-progress", line.to_string());
        };
        let ok = runtime::run_streaming(
            "sh",
            &[
                "-c",
                "apt-get update -y && apt-get install -y nodejs npm",
            ],
            &mut fwd,
        )
        .map(|s| s.success())
        .unwrap_or(false);
        if ok {
            return Ok("Node.js 已通过 apt 安装完成，重启应用后生效。".into());
        }
        let _ = tauri_plugin_opener::open_url("https://nodejs.org", None::<&str>);
        return Err("自动安装 Node.js 失败，已打开官方下载页，请手动安装后重启应用。".into());
    }
}

/// Update dsh to the configured version: stop → npm install → restart if it
/// was running.
#[tauri::command]
pub async fn update_dsh(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let was_running = matches!(
            app.state::<AppState>().manager.lock().unwrap().status,
            DshStatus::Running | DshStatus::Starting
        );
        let _ = manager::stop(&app);
        let version = app.state::<AppState>().settings.lock().unwrap().dsh_version.clone();
        manager::set_status(&app, DshStatus::Installing);
        let result = {
            let mut forward = runtime::forward_install_lines(&app);
            runtime::install(&app, &version, &mut forward)
        };
        match result {
            Ok(new_v) => {
                crate::logger::log_event(&app, "info", &format!("DSH 已更新：v{new_v}"));
                {
                    let state = app.state::<AppState>();
                    let mut st = state.manager.lock().unwrap();
                    st.installed_version = Some(new_v.clone());
                    st.status = DshStatus::InstalledIdle;
                }
                manager::emit_status(&app);
                if was_running {
                    let _ = manager::start(&app);
                }
                Ok(new_v)
            }
            Err(e) => {
                crate::logger::log_event(&app, "error", &format!("DSH 更新失败：{e}"));
                manager::set_status(&app, DshStatus::Error);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("更新任务异常: {e}"))?
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    app.state::<AppState>().settings.lock().unwrap().clone()
}

/// 持久化「首次向导已跳过/完成」，避免每次启动都弹出。
#[tauri::command]
pub fn dismiss_wizard(app: AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let mut s = state.settings.lock().unwrap();
        s.wizard_dismissed = true;
    }
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    settings::save(&app, &settings)?;
    crate::logger::log_event(&app, "info", "首次向导已跳过（持久化）");
    Ok(())
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    if settings.port == 0 {
        return Err("端口无效（1-65535）".into());
    }
    if let Some(cp) = settings.control_port {
        if cp == settings.port {
            return Err("控制端口不能与 DSH 端口相同".into());
        }
    }
    let control_changed = {
        let state = app.state::<AppState>();
        let mut s = state.settings.lock().unwrap();
        let changed = s.effective_control_port() != settings.effective_control_port();
        *s = settings.clone();
        changed
    };
    settings::save(&app, &settings)?;
    crate::logger::log_event(&app, "info", "设置已保存");
    // control port changed → re-bind the endpoint right away (no app restart)
    if control_changed {
        let a = app.clone();
        std::thread::spawn(move || {
            crate::control::start(&a);
        });
    }
    manager::emit_status(&app);
    Ok(())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch
            .enable()
            .map_err(|e| format!("设置开机启动失败: {e}"))?;
        crate::logger::log_event(&app, "info", "开机启动已开启，正在启动 DSH…");
        let a = app.clone();
        std::thread::spawn(move || {
            let _ = manager::start(&a);
        });
    } else {
        autolaunch
            .disable()
            .map_err(|e| format!("关闭开机启动失败: {e}"))?;
        crate::logger::log_event(&app, "info", "开机启动已关闭");
    }
    manager::emit_status(&app);
    Ok(autolaunch.is_enabled().unwrap_or(false))
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn get_logs(app: AppHandle, limit: Option<usize>) -> Vec<crate::logger::LogLine> {
    app.state::<AppState>()
        .log
        .lock()
        .unwrap()
        .tail(limit.unwrap_or(500))
}

#[tauri::command]
pub fn get_callback_info(app: AppHandle) -> CallbackInfo {
    let state = app.state::<AppState>();
    let effective = state.settings.lock().unwrap().effective_control_port();
    let control = *state.control_port.lock().unwrap();
    // show the actually-bound port; fall back to the configured one
    let shown = control.unwrap_or(effective);
    CallbackInfo {
        http_url: format!("http://127.0.0.1:{shown}/api/restart"),
        http_port: control,
        cli_cmd: "dsh-start restart".into(),
    }
}

#[tauri::command]
pub fn open_log_file(app: AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_log_dir()
        .map_err(|e| e.to_string())?
        .join("dsh-start.log");
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}
