use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::logger::LogRing;
use crate::settings::Settings;

/// dsh lifecycle status exposed to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DshStatus {
    /// Node.js is missing; guided install required.
    NodeMissing,
    /// npm install of dsh is in progress.
    Installing,
    /// dsh installed but not started.
    InstalledIdle,
    /// dsh child process spawned, waiting for readiness.
    Starting,
    /// dsh web UI is reachable.
    Running,
    /// dsh is reachable on the configured port but was started outside this
    /// app (not managed: no pid, no crash-restart).
    ExternalRunning,
    /// dsh stopped (intentionally).
    Stopped,
    /// dsh exited unexpectedly (or retries exhausted).
    Crashed,
    /// the configured port is occupied.
    PortInUse,
    /// generic error state.
    Error,
}

impl DshStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DshStatus::NodeMissing => "node-missing",
            DshStatus::Installing => "installing",
            DshStatus::InstalledIdle => "installed-idle",
            DshStatus::Starting => "starting",
            DshStatus::Running => "running",
            DshStatus::ExternalRunning => "external",
            DshStatus::Stopped => "stopped",
            DshStatus::Crashed => "crashed",
            DshStatus::PortInUse => "port-in-use",
            DshStatus::Error => "error",
        }
    }
}

pub struct ManagerState {
    pub status: DshStatus,
    pub pid: Option<u32>,
    pub installed_version: Option<String>,
    pub started_at: Option<Instant>,
    pub last_error: Option<String>,
    /// crash auto-restart retries left this session.
    pub crash_retries_left: u32,
    pub last_restart_at: Option<Instant>,
    /// set when we intentionally stop the child (suppresses crash handling).
    pub intentional_stop: Arc<AtomicBool>,
    /// set while a spawned child process is still alive.
    pub child_alive: Arc<AtomicBool>,
    /// set when the child's stderr reported an address-in-use error.
    pub addr_in_use: Arc<AtomicBool>,
}

impl Default for ManagerState {
    fn default() -> Self {
        Self {
            status: DshStatus::InstalledIdle,
            pid: None,
            installed_version: None,
            started_at: None,
            last_error: None,
            crash_retries_left: 0,
            last_restart_at: None,
            intentional_stop: Arc::new(AtomicBool::new(false)),
            child_alive: Arc::new(AtomicBool::new(false)),
            addr_in_use: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub node: PathBuf,
    pub npm_cli: Option<PathBuf>,
    pub node_version: String,
}

pub struct AppState {
    pub manager: Mutex<ManagerState>,
    pub settings: Mutex<Settings>,
    pub log: Mutex<LogRing>,
    pub node_info: Mutex<Option<NodeInfo>>,
    /// actually-bound control endpoint port (None = HTTP callback unavailable).
    pub control_port: Mutex<Option<u16>>,
    /// bumped on every control-endpoint (re)bind so the previous listener exits.
    pub control_generation: AtomicU64,
    /// npm 写操作（安装/更新/系统升级）互斥标志：并发的 npm/npx 会争抢同一份
    /// 缓存锁（libnpmexec 报 ECOMPROMISED / Lock compromised），同一时刻只放行一个。
    pub install_busy: AtomicBool,
    pub shim_dir: Mutex<Option<PathBuf>>,
    pub tray: Mutex<Option<tauri::tray::TrayIcon>>,
    pub tray_status_item: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    /// lifecycle menu items, enabled/disabled per status in tray::update.
    pub tray_items: Mutex<Option<crate::tray::TrayItems>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            manager: Mutex::new(ManagerState::default()),
            settings: Mutex::new(Settings::default()),
            log: Mutex::new(LogRing::empty()),
            node_info: Mutex::new(None),
            control_port: Mutex::new(None),
            control_generation: AtomicU64::new(0),
            install_busy: AtomicBool::new(false),
            shim_dir: Mutex::new(None),
            tray: Mutex::new(None),
            tray_status_item: Mutex::new(None),
            tray_items: Mutex::new(None),
        }
    }
}
