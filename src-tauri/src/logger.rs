use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub const RING_CAPACITY: usize = 2000;
const MAX_LOG_FILE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_LOG_FILES: u32 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub ts: String,
    pub level: String,
    pub msg: String,
}

/// In-memory ring buffer + rotating file writer for all dsh-start logging.
pub struct LogRing {
    lines: VecDeque<LogLine>,
    file: Option<(PathBuf, File, u64)>,
}

impl LogRing {
    pub fn empty() -> Self {
        Self {
            lines: VecDeque::with_capacity(RING_CAPACITY),
            file: None,
        }
    }

    pub fn new(app: &AppHandle) -> Self {
        let path = app
            .path()
            .app_log_dir()
            .ok()
            .map(|d| d.join("dsh-start.log"));
        let file = path.and_then(|p| {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&p)
                .ok()?;
            let bytes = f.metadata().map(|m| m.len()).unwrap_or(0);
            Some((p, f, bytes))
        });
        Self {
            lines: VecDeque::with_capacity(RING_CAPACITY),
            file,
        }
    }

    fn rotate(&mut self) {
        if self.file.is_none() {
            return;
        }
        let (path, f, _) = self.file.take().unwrap();
        drop(f); // close before renaming (Windows lock)
        for idx in (2..=MAX_LOG_FILES).rev() {
            let src = path.with_extension(format!("log.{}", idx - 1));
            let dst = path.with_extension(format!("log.{}", idx));
            let _ = std::fs::rename(&src, &dst);
        }
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
        if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
            self.file = Some((path, f, 0));
        }
    }

    pub fn push(&mut self, level: &str, msg: &str) -> LogLine {
        let ts = chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S%.3f")
            .to_string();
        let line = LogLine {
            ts,
            level: level.to_string(),
            msg: msg.to_string(),
        };
        self.lines.push_back(line.clone());
        while self.lines.len() > RING_CAPACITY {
            self.lines.pop_front();
        }
        let text = format!("{} [{}] {}\n", line.ts, line.level, line.msg);
        if let Some((_, _, bytes)) = &self.file {
            if *bytes + text.len() as u64 > MAX_LOG_FILE_BYTES {
                self.rotate();
            }
        }
        if let Some((_, f, bytes)) = &mut self.file {
            let _ = f.write_all(text.as_bytes());
            *bytes += text.len() as u64;
            let _ = f.flush();
        }
        line
    }

    pub fn tail(&self, limit: usize) -> Vec<LogLine> {
        self.lines
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

/// Log a line into the ring buffer + file and emit it to the UI.
pub fn log_event(app: &AppHandle, level: &str, msg: &str) {
    let line = match app.try_state::<crate::state::AppState>() {
        Some(state) => state.log.lock().unwrap().push(level, msg),
        None => LogLine {
            ts: chrono::Local::now()
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string(),
            level: level.to_string(),
            msg: msg.to_string(),
        },
    };
    let _ = app.emit("dsh-log", line);
    if cfg!(debug_assertions) {
        eprintln!("[{level}] {msg}");
    }
}

/// Bridges the `log` crate into the app log ring.
pub struct AppLogger(pub AppHandle);

impl log::Log for AppLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        log_event(&self.0, record.level().as_str(), &record.args().to_string());
    }
    fn flush(&self) {}
}
