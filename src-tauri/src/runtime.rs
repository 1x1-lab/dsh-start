use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;

use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, NodeInfo};

pub const DSH_PACKAGE: &str = "@deepseek-ai/dsh";
pub const DSH_BIN_REL: &str = "node_modules/@deepseek-ai/dsh/lib/bin.js";

/// PATH lookup with Windows extension handling (.exe/.cmd/.bat).
pub fn find_on_path(program: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var_os("PATHEXT")
            .map(|p| {
                std::env::split_paths(&p)
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .collect()
            })
            .unwrap_or_else(|| vec![".exe".into(), ".cmd".into(), ".bat".into()])
    } else {
        vec![]
    };
    for dir in std::env::split_paths(&path_var) {
        let base = dir.join(program);
        // Windows：先按 PATHEXT 依次匹配（与 cmd 同语义），再试无扩展名文件。
        // nodejs 目录下同时存在 POSIX sh 脚本（如 npx/npm）与 .cmd，必须优先 .cmd，
        // 否则会拿到无法直接执行的 sh 脚本。
        for ext in &exts {
            let cand = PathBuf::from(format!("{}{}", base.to_string_lossy(), ext));
            if cand.is_file() {
                return Some(cand);
            }
        }
        if base.is_file() {
            return Some(base);
        }
    }
    None
}

/// Run a command capturing combined output (fast, small outputs only).
pub fn run_capture(exe: &Path, args: &[&str], cwd: Option<&Path>) -> Result<(i32, String), String> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("无法执行 {}: {e}", exe.display()))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok((out.status.code().unwrap_or(-1), text))
}

pub fn detect_node() -> Result<NodeInfo, String> {
    let node = find_on_path("node").ok_or("未检测到 Node.js，请先安装 Node.js 18+（https://nodejs.org）")?;
    let (code, out) = run_capture(&node, &["--version"], None)?;
    let node_version = out.trim().to_string();
    if code != 0 {
        return Err(format!("node --version 失败: {out}"));
    }
    let npm_cli = resolve_npm_cli(&node);
    Ok(NodeInfo {
        node,
        npm_cli,
        node_version,
    })
}

/// Resolve the npm-cli.js entry so we can run npm through `node` directly
/// (no .cmd shim / shell indirection needed).
fn resolve_npm_cli(node: &Path) -> Option<PathBuf> {
    // official installs keep npm under the node directory
    let sibling = node
        .parent()
        .map(|d| d.join("node_modules/npm/bin/npm-cli.js"))
        .filter(|p| p.is_file());
    if sibling.is_some() {
        return sibling;
    }
    // package-manager installs (brew/apt/…): `npm` resolves (symlink) to npm-cli.js
    if let Some(npm) = find_on_path("npm") {
        if let Ok(canon) = std::fs::canonicalize(&npm) {
            if canon.to_string_lossy().ends_with("npm-cli.js") {
                return Some(canon);
            }
            // Windows .cmd shim: npm-cli.js lives next to node
            if cfg!(windows) {
                let p = canon
                    .parent()
                    .map(|d| d.join("node_modules/npm/bin/npm-cli.js"))
                    .filter(|p| p.is_file());
                if p.is_some() {
                    return p;
                }
            }
        }
    }
    None
}

/// Resolve the npx-cli.js entry so we can run npx through `node` directly
/// (no .cmd shim / shell indirection needed).
fn resolve_npx_cli(node: &Path) -> Option<PathBuf> {
    node.parent()
        .map(|d| d.join("node_modules/npm/bin/npx-cli.js"))
        .filter(|p| p.is_file())
}

pub fn runtime_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_default()
        .join("runtime")
}

pub fn installed_version(app: &AppHandle) -> Option<String> {
    let pkg = runtime_dir(app).join("node_modules/@deepseek-ai/dsh/package.json");
    let text = std::fs::read_to_string(pkg).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn looks_like_version(s: &str) -> bool {
    let t = s.trim();
    let mut chars = t.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_digit()) && t.contains('.')
}

fn npx_cache_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA")
        .map(|p| Path::new(&p).join("npm-cache/_npx"));
    #[cfg(not(windows))]
    let root = std::env::var_os("HOME").map(|p| Path::new(&p).join(".npm/_npx"));
    if let Some(root) = root {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for e in entries.flatten() {
                dirs.push(e.path());
            }
        }
    }
    dirs
}

/// 按 . / - 分段比较版本号；a>b 返回 Greater。
fn cmp_ver(a: &str, b: &str) -> std::cmp::Ordering {
    let pa: Vec<u32> = a
        .split(|c: char| c == '.' || c == '-')
        .filter_map(|s| s.parse().ok())
        .collect();
    let pb: Vec<u32> = b
        .split(|c: char| c == '.' || c == '-')
        .filter_map(|s| s.parse().ok())
        .collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return x.cmp(&y);
        }
    }
    std::cmp::Ordering::Equal
}

/// 检测系统层面是否已存在 dsh（PATH 上的 `dsh` 命令 / npm npx 缓存），
/// 返回找到的最高版本号。应用自身托管目录的检查见 [`installed_version`]。
/// 用途：电脑上已有 dsh 时不再误报「未安装」。
pub fn system_dsh_version() -> Option<String> {
    let mut found: Vec<String> = Vec::new();
    // 1) PATH 上的 dsh 命令（全局安装等）
    if let Some(dsh) = find_on_path("dsh") {
        if let Ok((code, out)) = run_capture(&dsh, &["--version"], None) {
            let v = out.trim().to_string();
            if code == 0 && looks_like_version(&v) {
                found.push(v);
            }
        }
    }
    // 2) npm npx 缓存中的 @deepseek-ai/dsh（可能多份，取最高版本）
    for dir in npx_cache_dirs() {
        let pkg = dir.join("node_modules/@deepseek-ai/dsh/package.json");
        if let Ok(text) = std::fs::read_to_string(&pkg) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(ver) = v.get("version").and_then(|x| x.as_str()) {
                    found.push(ver.to_string());
                }
            }
        }
    }
    found.into_iter().max_by(|a, b| cmp_ver(a, b))
}

/// 从系统 npx 缓存中定位 dsh 可执行入口与运行目录（供托管缺失时直接使用）。
/// 返回 (bin.js 路径, cwd=node_modules 所在缓存目录)。取最高版本的缓存。
pub fn system_dsh_dir() -> Option<(PathBuf, PathBuf)> {
    let mut best: Option<(String, PathBuf, PathBuf)> = None; // (version, bin, cwd)
    for dir in npx_cache_dirs() {
        let pkg_dir = dir.join("node_modules/@deepseek-ai/dsh");
        let bin = pkg_dir.join("lib/bin.js");
        if !bin.is_file() {
            continue;
        }
        let pkg = pkg_dir.join("package.json");
        let ver = std::fs::read_to_string(&pkg)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("version").and_then(|x| x.as_str()).map(String::from))
            .unwrap_or_default();
        let is_better = best
            .as_ref()
            .map(|(v, _, _)| cmp_ver(&ver, v) == std::cmp::Ordering::Greater)
            .unwrap_or(true);
        if is_better {
            best = Some((ver, bin, dir));
        }
    }
    best.map(|(_, bin, dir)| (bin, dir))
}

/// 升级系统（非托管）安装的 dsh，按它原本的安装方式原地升级：
/// - PATH 上的全局安装 → `npm install -g @deepseek-ai/dsh@<version>`
/// - npx 缓存 → `npx --yes @deepseek-ai/dsh@<version> --version` 刷新缓存
/// 返回升级后的版本号。不会把系统安装转换为托管副本。
pub fn upgrade_system_dsh(app: &AppHandle, version: &str) -> Result<String, String> {
    let node_info = detect_node()?;
    let spec = format!("{}@{}", DSH_PACKAGE, version);
    let mut forward = forward_install_lines(app);

    if let Some(dsh) = find_on_path("dsh") {
        // 全局安装：npm install -g
        crate::logger::log_event(
            app,
            "info",
            &format!("升级系统 DSH（全局安装）到 {version}…"),
        );
        let args = [
            "install",
            "-g",
            &spec,
            "--no-audit",
            "--no-fund",
            "--color=false",
        ];
        let status = if let Some(cli) = &node_info.npm_cli {
            let mut cmd = Command::new(&node_info.node);
            cmd.arg(cli).args(args);
            run_cmd_streaming(&mut cmd, &mut forward)?
        } else if cfg!(windows) {
            let npm = find_on_path("npm").ok_or("未检测到 npm")?;
            let mut cmd = Command::new("cmd");
            cmd.arg("/C")
                .arg(format!("\"{}\"", npm.display()))
                .args(args);
            run_cmd_streaming(&mut cmd, &mut forward)?
        } else {
            let npm = find_on_path("npm").ok_or("未检测到 npm")?;
            let mut cmd = Command::new(npm);
            cmd.args(args);
            run_cmd_streaming(&mut cmd, &mut forward)?
        };
        if !status.success() {
            return Err(format!("npm install -g 失败（exit={:?}）", status.code()));
        }
        // 升级后读回全局版本
        let (code, out) = run_capture(&dsh, &["--version"], None)?;
        let v = out.trim().to_string();
        if code == 0 && looks_like_version(&v) {
            return Ok(v);
        }
        return Err("升级完成但未从全局 dsh 读到版本号".into());
    }

    // npx 缓存：npx --yes @deepseek-ai/dsh@<v> --version 刷新到最新
    crate::logger::log_event(
        app,
        "info",
        &format!("升级系统 DSH（npx 缓存）到 {version}…"),
    );
    let args = ["--yes", &spec, "--version"];
    // 优先 node + npx-cli.js 直跑，绕开 .cmd shim 与 cmd.exe 的引号转义坑
    // （手动加的引号会被 Rust 再转义成 \" ，cmd 解析后命令行直接损坏）。
    let status = if let Some(cli) = resolve_npx_cli(&node_info.node) {
        let mut cmd = Command::new(&node_info.node);
        cmd.arg(cli).args(args);
        run_cmd_streaming(&mut cmd, &mut forward)?
    } else {
        let npx = find_on_path("npx").ok_or("未检测到 npx")?;
        if cfg!(windows) {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C")
                .arg(format!("\"{}\"", npx.display()))
                .args(args);
            run_cmd_streaming(&mut cmd, &mut forward)?
        } else {
            let mut cmd = Command::new(npx);
            cmd.args(args);
            run_cmd_streaming(&mut cmd, &mut forward)?
        }
    };
    if !status.success() {
        return Err(format!("npx 刷新失败（exit={:?}）", status.code()));
    }
    system_dsh_version().ok_or_else(|| "升级后未在系统检测到 DSH 版本号".into())
}

pub fn dsh_bin(app: &AppHandle) -> Option<PathBuf> {
    let p = runtime_dir(app).join(DSH_BIN_REL);
    p.is_file().then_some(p)
}

fn ensure_package_json(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let pkg = dir.join("package.json");
    if !pkg.exists() {
        let content = serde_json::json!({
            "name": "dsh-start-runtime",
            "private": true,
            "version": "0.0.0",
            "description": "Managed dsh runtime directory (auto-installed by dsh-start)"
        });
        std::fs::write(&pkg, serde_json::to_string_pretty(&content).unwrap())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Strip ANSI escape sequences from a text line (npm/installer output).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            while let Some(&n) = chars.peek() {
                chars.next();
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn run_cmd_streaming(
    cmd: &mut Command,
    on_line: &mut dyn FnMut(&str),
) -> Result<ExitStatus, String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("进程启动失败: {e}"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, rx) = mpsc::channel::<String>();
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if let Ok(l) = line {
                    let _ = tx.send(l);
                }
            }
        });
    }
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                if let Ok(l) = line {
                    let _ = tx.send(l);
                }
            }
        });
    }
    drop(tx);
    while let Ok(line) = rx.recv() {
        on_line(&line);
    }
    child.wait().map_err(|e| e.to_string())
}

/// Run a generic command streaming its output lines to `on_line`.
pub fn run_streaming(
    exe: &str,
    args: &[&str],
    on_line: &mut dyn FnMut(&str),
) -> Result<ExitStatus, String> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    run_cmd_streaming(&mut cmd, on_line)
}

/// Run npm (through node + npm-cli.js when possible) streaming output lines.
pub fn run_npm(
    node: &Path,
    npm_cli: Option<&Path>,
    args: &[&str],
    cwd: &Path,
    on_line: &mut dyn FnMut(&str),
) -> Result<ExitStatus, String> {
    let mut cmd = if let Some(cli) = npm_cli {
        let mut c = Command::new(node);
        c.arg(cli);
        c
    } else if cfg!(windows) {
        let npm = find_on_path("npm").ok_or("未检测到 npm")?;
        let mut c = Command::new("cmd");
        c.arg("/C").arg(format!("\"{}\"", npm.display()));
        c
    } else {
        let npm = find_on_path("npm").ok_or("未检测到 npm")?;
        let c = Command::new(npm);
        c
    };
    cmd.args(args)
        .current_dir(cwd)
        .env("npm_config_progress", "false")
        .env("NO_COLOR", "1");
    run_cmd_streaming(&mut cmd, on_line)
}

/// Install (or update) dsh into the managed runtime dir. Returns the installed
/// version on success.
pub fn install(app: &AppHandle, version: &str, on_line: &mut dyn FnMut(&str)) -> Result<String, String> {
    let node_info = detect_node()?;
    let dir = runtime_dir(app);
    ensure_package_json(&dir)?;
    let spec = format!("{}@{}", DSH_PACKAGE, version);
    let args = [
        "install",
        "--prefix",
        dir.to_str().unwrap_or("."),
        "--no-audit",
        "--no-fund",
        "--loglevel",
        "info",
        "--color=false",
        &spec,
    ];
    let status = run_npm(&node_info.node, node_info.npm_cli.as_deref(), &args, &dir, on_line)?;
    if !status.success() {
        return Err(format!("npm install 失败（exit={:?}）", status.code()));
    }
    installed_version(app).ok_or_else(|| "安装完成但未找到 DSH 版本号".into())
}

/// Resolve the configured dsh version spec (`latest` or a pinned semver)
/// against the npm registry. Returns the concrete version it would install.
pub fn latest_version(app: &AppHandle) -> Result<String, String> {
    let node_info = detect_node()?;
    let spec_setting = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .dsh_version
        .clone();
    let dir = runtime_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let spec = format!("{}@{}", DSH_PACKAGE, spec_setting);
    let args = ["view", &spec, "version", "--loglevel", "error", "--color=false"];
    let mut last = String::new();
    let status = run_npm(
        &node_info.node,
        node_info.npm_cli.as_deref(),
        &args,
        &dir,
        &mut |line| {
            let clean = strip_ansi(line);
            let t = clean.trim();
            if !t.is_empty() {
                last = t.to_string();
            }
        },
    )?;
    if !status.success() {
        return Err(format!("npm view 失败（exit={:?}）", status.code()));
    }
    if last.is_empty() {
        return Err("未从 npm 获取到 DSH 版本号".into());
    }
    Ok(last)
}

/// Convenience used by long-running commands: log + forward npm lines.
pub fn forward_install_lines(app: &AppHandle) -> impl FnMut(&str) + '_ {
    move |line: &str| {
        let clean = strip_ansi(line);
        if !clean.trim().is_empty() {
            crate::logger::log_event(app, "info", &format!("[npm] {}", clean.trim()));
            let _ = app.emit("install-progress", clean.trim().to_string());
        }
    }
}

#[allow(dead_code)]
pub fn cache_node_info(app: &AppHandle) {
    *app.state::<AppState>().node_info.lock().unwrap() = detect_node().ok();
}
