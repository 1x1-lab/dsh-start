use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, Default)]
pub struct CliFlags {
    /// invoked as `dsh-start restart` (callback restart request).
    pub restart: bool,
    /// launched hidden (used by OS autostart).
    pub minimized: bool,
}

pub fn flags() -> CliFlags {
    let args: Vec<String> = std::env::args().collect();
    CliFlags {
        restart: args.iter().any(|a| a == "restart"),
        minimized: args.iter().any(|a| a == "--minimized"),
    }
}

pub fn shim_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_default().join("bin")
}

/// Write the `dsh-start` callback shim so that dsh's own shell tools can
/// trigger a restart without any user input.
pub fn register(app: &AppHandle) -> Result<(), String> {
    let dir = shim_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;

    #[cfg(windows)]
    {
        let shim = dir.join("dsh-start.cmd");
        let content = format!(
            "@echo off\r\nif defined DSH_START_EXE (start \"\" \"%DSH_START_EXE%\" restart) else (start \"\" \"{}\" restart)\r\n",
            exe.display()
        );
        std::fs::write(&shim, content).map_err(|e| e.to_string())?;
        add_to_user_path(&dir)?;
    }

    #[cfg(unix)]
    {
        let shim = dir.join("dsh-start");
        let content = format!(
            "#!/bin/sh\nexec \"${{DSH_START_EXE:-{}}}\" restart \"$@\"\n",
            exe.display()
        );
        std::fs::write(&shim, content).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755));
        }
        let home = std::env::var("HOME").unwrap_or_default();
        for d in [format!("{home}/.local/bin"), format!("{home}/bin")] {
            let _ = std::fs::create_dir_all(&d);
            let target = Path::new(&d).join("dsh-start");
            let _ = std::fs::remove_file(&target);
            let _ = std::os::unix::fs::symlink(&shim, &target);
        }
    }

    crate::logger::log_event(
        app,
        "info",
        &format!("已注册回调命令：{}（DSH 内可直接执行 dsh-start restart）", dir.display()),
    );
    Ok(())
}

/// Remove the callback shim and PATH entries.
pub fn unregister(app: &AppHandle) {
    #[cfg(windows)]
    {
        remove_from_user_path(&shim_dir(app));
        let _ = std::fs::remove_file(shim_dir(app).join("dsh-start.cmd"));
    }
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(shim_dir(app).join("dsh-start"));
        let home = std::env::var("HOME").unwrap_or_default();
        for d in [format!("{home}/.local/bin"), format!("{home}/bin")] {
            let _ = std::fs::remove_file(Path::new(&d).join("dsh-start"));
        }
    }
}

#[cfg(windows)]
fn get_user_path() -> Option<String> {
    let out = std::process::Command::new("reg")
        .args(["query", "HKCU\\Environment", "/v", "Path"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    for line in text.lines() {
        let t = line.trim();
        let mut parts = t.split_whitespace();
        if parts.next() == Some("Path") {
            let _ty = parts.next();
            let value = parts.collect::<Vec<_>>().join(" ");
            return Some(value);
        }
    }
    None
}

#[cfg(windows)]
fn set_user_path(value: &str) -> Result<(), String> {
    let status = std::process::Command::new("reg")
        .args([
            "add",
            "HKCU\\Environment",
            "/v",
            "Path",
            "/t",
            "REG_EXPAND_SZ",
            "/d",
            value,
            "/f",
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("reg add 失败".into())
    }
}

#[cfg(windows)]
fn add_to_user_path(dir: &Path) -> Result<(), String> {
    let dir_str = dir.to_string_lossy().trim_end_matches('\\').to_string();
    let current = get_user_path().unwrap_or_default();
    if current
        .split(';')
        .any(|p| p.trim_end_matches('\\') == dir_str)
    {
        return Ok(());
    }
    let sep = if current.is_empty() { "" } else { ";" };
    set_user_path(&format!("{current}{sep}{dir_str}"))
}

#[cfg(windows)]
fn remove_from_user_path(dir: &Path) {
    let dir_str = dir.to_string_lossy().trim_end_matches('\\').to_string();
    if let Some(current) = get_user_path() {
        let parts: Vec<&str> = current
            .split(';')
            .filter(|p| !p.is_empty() && p.trim_end_matches('\\') != dir_str)
            .collect();
        let _ = set_user_path(&parts.join(";"));
    }
}
