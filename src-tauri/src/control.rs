use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::manager;
use crate::state::AppState;

pub const MAX_BODY: usize = 4096;
/// how many ports past the configured one to try when it is occupied.
const FALLBACK_SPAN: u16 = 10;

/// Bind (or re-bind) the localhost control endpoint and serve
/// `GET /api/status` and `POST /api/restart` with CORS restricted to the dsh
/// web origin. The port comes from settings (custom override, or dsh port + 1);
/// if it is occupied, the next few ports are scanned. The actual bound port is
/// stored in state and pushed to the UI; None means every candidate failed.
pub fn start(app: &AppHandle) -> Option<u16> {
    let desired = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .effective_control_port();

    // supersede any previous listener: bump the generation, then wake its
    // blocking accept() with a dummy connection so it notices and drops the
    // socket (releasing the port for us).
    let state = app.state::<AppState>();
    let prev = state.control_port.lock().unwrap().take();
    let gen = state.control_generation.fetch_add(1, Ordering::SeqCst) + 1;
    if let Some(p) = prev {
        let _ = TcpStream::connect(std::net::SocketAddr::from(([127, 0, 0, 1], p)));
    }

    let mut bound: Option<(u16, TcpListener)> = None;
    let mut last_err = String::new();
    'scan: for offset in 0..=FALLBACK_SPAN {
        let port = desired.saturating_add(offset);
        if port == 0 {
            continue;
        }
        // the previous listener may need a few ms to release the same port
        let attempts = if offset == 0 { 20 } else { 1 };
        for _ in 0..attempts {
            match TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], port))) {
                Ok(l) => {
                    bound = Some((port, l));
                    break 'scan;
                }
                Err(e) => {
                    last_err = e.to_string();
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }

    let Some((port, listener)) = bound else {
        crate::logger::log_event(
            app,
            "warn",
            &format!(
                "控制端点绑定失败（{desired} 及后续 {FALLBACK_SPAN} 个端口均被占用，HTTP 回调不可用）: {last_err}"
            ),
        );
        manager::emit_status(app);
        return None;
    };

    *app.state::<AppState>().control_port.lock().unwrap() = Some(port);
    if port == desired {
        crate::logger::log_event(
            app,
            "info",
            &format!("控制端点已启动：http://127.0.0.1:{port}（回调重启可用）"),
        );
    } else {
        crate::logger::log_event(
            app,
            "warn",
            &format!("控制端口 {desired} 被占用，已回退到 http://127.0.0.1:{port}"),
        );
    }
    manager::emit_status(app);

    let app = app.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            // a newer (re)bind supersedes this listener → exit and release
            if app
                .state::<AppState>()
                .control_generation
                .load(Ordering::SeqCst)
                != gen
            {
                break;
            }
            match stream {
                Ok(s) => {
                    let a = app.clone();
                    std::thread::spawn(move || handle(&a, s));
                }
                Err(_) => break,
            }
        }
    });
    Some(port)
}

fn handle(app: &AppHandle, mut stream: TcpStream) {
    let start = Instant::now();
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut buf = Vec::with_capacity(2048);
    let mut tmp = [0u8; 2048];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.len() > MAX_BODY || buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = text.lines();
    let request_line = lines.next().unwrap_or("").to_string();

    let mut origin: Option<String> = None;
    let mut content_length = 0usize;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("origin:") {
            origin = Some(line[7..].trim().to_string());
        } else if lower.starts_with("content-length:") {
            // "content-length:" 共 15 字符，从下标 15 起才是数值
            content_length = line[15..].trim().parse().unwrap_or(0);
        }
        if line.is_empty() {
            break;
        }
    }

    let method = request_line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_uppercase();
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();

    let dsh_port = app.state::<AppState>().settings.lock().unwrap().port;
    let allowed_origin = format!("http://127.0.0.1:{dsh_port}");
    let allowed_origin2 = format!("http://localhost:{dsh_port}");
    let cors_ok = origin
        .as_deref()
        .map(|o| o == allowed_origin || o == allowed_origin2)
        .unwrap_or(false);

    let mut body = String::new();
    if content_length > 0 {
        let header_end = text.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
        body = text[header_end..].chars().take(content_length).collect();
    }

    let mut response = String::new();
    let status_code: u16;
    let mut note = String::new();
    match (method.as_str(), path.as_str()) {
        ("OPTIONS", _) => {
            status_code = 204;
            response.push_str("HTTP/1.1 204 No Content\r\n");
            if cors_ok {
                if let Some(o) = &origin {
                    response.push_str(&format!("Access-Control-Allow-Origin: {o}\r\n"));
                }
            }
            response.push_str(
                "Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                 Access-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\n\r\n",
            );
        }
        ("GET", "/api/status") => {
            status_code = 200;
            let payload = manager::status_payload(app);
            let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
            response.push_str("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n");
            if cors_ok {
                if let Some(o) = &origin {
                    response.push_str(&format!("Access-Control-Allow-Origin: {o}\r\n"));
                }
            }
            response.push_str(&format!(
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                json.len(),
                json
            ));
        }
        ("POST", "/api/restart") => {
            status_code = 200;
            let reason = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("reason")
                        .and_then(|r| r.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "http-callback".into());
            note = format!(" reason=\"{reason}\"");
            let app2 = app.clone();
            std::thread::spawn(move || {
                let _ = manager::restart(&app2, &reason);
            });
            let json = r#"{"ok":true}"#;
            response.push_str("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n");
            if cors_ok {
                if let Some(o) = &origin {
                    response.push_str(&format!("Access-Control-Allow-Origin: {o}\r\n"));
                }
            }
            response.push_str(&format!(
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                json.len(),
                json
            ));
        }
        _ => {
            status_code = 404;
            let json = r#"{"error":"not found"}"#;
            response.push_str("HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n");
            if cors_ok {
                if let Some(o) = &origin {
                    response.push_str(&format!("Access-Control-Allow-Origin: {o}\r\n"));
                }
            }
            response.push_str(&format!(
                "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                json.len(),
                json
            ));
        }
    }
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    // 请求日志：方法 / 路径 / 状态码 / 可选原因 / 耗时（空请求（读超时）不记）
    if !method.is_empty() {
        crate::logger::log_event(
            app,
            "info",
            &format!(
                "[http] {method} {path} {status_code}{note} ({}ms)",
                start.elapsed().as_millis()
            ),
        );
    }
}
