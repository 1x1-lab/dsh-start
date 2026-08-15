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
        if base.is_file() {
            return Some(base);
        }
        for ext in &exts {
            let cand = PathBuf::from(format!("{}{}", base.to_string_lossy(), ext));
            if cand.is_file() {
                return Some(cand);
            }
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
