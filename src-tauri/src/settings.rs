use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub const DEFAULT_DSH_PORT: u16 = 3080;
pub const DEFAULT_DSH_VERSION: &str = "latest";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// port the dsh web UI listens on (default 3080).
    pub port: u16,
    /// custom control/callback port; None = auto (dsh port + 1).
    pub control_port: Option<u16>,
    /// dsh version to install / keep (`latest` or a semver).
    pub dsh_version: String,
    /// auto-restart dsh after an unexpected exit while the app runs.
    pub crash_restart: bool,
    /// stop dsh when the launcher app quits.
    pub quit_stops_dsh: bool,
    /// register the `dsh-start` callback command into PATH (and into the dsh
    /// child's PATH) so dsh's own shell tools can trigger a restart.
    pub register_cli: bool,
    /// UI / tray language: "zh" (default) or "en".
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: DEFAULT_DSH_PORT,
            control_port: None,
            dsh_version: DEFAULT_DSH_VERSION.to_string(),
            crash_restart: true,
            quit_stops_dsh: true,
            register_cli: true,
            language: "zh".to_string(),
        }
    }
}

impl Settings {
    /// Effective control endpoint port: custom override, or dsh port + 1.
    pub fn effective_control_port(&self) -> u16 {
        self.control_port
            .filter(|p| *p > 0)
            .unwrap_or_else(|| self.port.saturating_add(1))
    }
}

pub fn settings_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_default()
        .join("settings.json")
}

pub fn load(app: &AppHandle) -> Settings {
    let path = settings_path(app);
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            // tolerate a UTF-8 BOM written by some editors/scripts
            let s = s.trim_start_matches('\u{feff}');
            serde_json::from_str(s).unwrap_or_default()
        }
        Err(_) => Settings::default(),
    }
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}
