use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::manager;
use crate::state::{AppState, DshStatus};

/// Menu items retexted (language) and enabled/disabled (status) in `update`.
pub struct TrayItems {
    pub start: MenuItem<tauri::Wry>,
    pub stop: MenuItem<tauri::Wry>,
    pub restart: MenuItem<tauri::Wry>,
    pub open: MenuItem<tauri::Wry>,
    pub open_web: MenuItem<tauri::Wry>,
    pub quit: MenuItem<tauri::Wry>,
}

struct TrayText {
    start: &'static str,
    stop: &'static str,
    restart: &'static str,
    open: &'static str,
    open_web: &'static str,
    quit: &'static str,
    status_prefix: &'static str,
}

fn tray_text(lang: &str) -> TrayText {
    match lang {
        "en" => TrayText {
            start: "Start DSH",
            stop: "Stop DSH",
            restart: "Restart DSH",
            open: "Open Console",
            open_web: "Open DSH in Browser",
            quit: "Quit",
            status_prefix: "DSH: ",
        },
        _ => TrayText {
            start: "启动 DSH",
            stop: "停止 DSH",
            restart: "重启 DSH",
            open: "打开控制台",
            open_web: "在浏览器打开 DSH",
            quit: "退出",
            status_prefix: "DSH：",
        },
    }
}

fn status_label(s: DshStatus, lang: &str) -> &'static str {
    if lang == "en" {
        return match s {
            DshStatus::NodeMissing => "Node.js Missing",
            DshStatus::Installing => "Installing",
            DshStatus::InstalledIdle => "Idle",
            DshStatus::Starting => "Starting",
            DshStatus::Running => "Running",
            DshStatus::ExternalRunning => "Running · External",
            DshStatus::Stopped => "Stopped",
            DshStatus::Crashed => "Crashed",
            DshStatus::PortInUse => "Port In Use",
            DshStatus::Error => "Error",
        };
    }
    match s {
        DshStatus::NodeMissing => "缺少 Node.js",
        DshStatus::Installing => "安装中",
        DshStatus::InstalledIdle => "未启动",
        DshStatus::Starting => "启动中",
        DshStatus::Running => "运行中",
        DshStatus::ExternalRunning => "运行中 · 外部实例",
        DshStatus::Stopped => "已停止",
        DshStatus::Crashed => "已崩溃",
        DshStatus::PortInUse => "端口被占用",
        DshStatus::Error => "出错",
    }
}

fn current_lang(app: &AppHandle) -> String {
    app.state::<AppState>().settings.lock().unwrap().language.clone()
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let text = tray_text(&current_lang(app));
    let status_item = MenuItem::with_id(app, "status", "DSH：—", false, None::<&str>)?;
    let start_i = MenuItem::with_id(app, "start", text.start, true, None::<&str>)?;
    let stop_i = MenuItem::with_id(app, "stop", text.stop, true, None::<&str>)?;
    let restart_i = MenuItem::with_id(app, "restart", text.restart, true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let open_i = MenuItem::with_id(app, "open", text.open, true, None::<&str>)?;
    let open_web_i = MenuItem::with_id(app, "open-web", text.open_web, true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit_i = MenuItem::with_id(app, "quit", text.quit, true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &start_i,
            &stop_i,
            &restart_i,
            &sep1,
            &open_i,
            &open_web_i,
            &sep2,
            &quit_i,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().expect("window icon").clone())
        .tooltip("DSH-start")
        .menu(&menu)
        .show_menu_on_left_click(false)
        // 左键单击：可见且聚焦 → 最小化到托盘；否则唤出到前台
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let showing = w.is_visible().unwrap_or(false)
                        && !w.is_minimized().unwrap_or(false);
                    let focused = w.is_focused().unwrap_or(false);
                    if showing && focused {
                        let _ = w.hide();
                    } else {
                        show_main(app);
                    }
                }
            }
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "start" => {
                let a = app.clone();
                std::thread::spawn(move || {
                    let _ = manager::start(&a);
                });
            }
            "stop" => {
                let a = app.clone();
                std::thread::spawn(move || {
                    let _ = manager::stop(&a);
                });
            }
            "restart" => {
                let a = app.clone();
                std::thread::spawn(move || {
                    let _ = manager::restart(&a, "tray");
                });
            }
            "open" => show_main(app),
            "open-web" => {
                let a = app.clone();
                std::thread::spawn(move || manager::open_web(&a));
            }
            "quit" => {
                let a = app.clone();
                std::thread::spawn(move || {
                    let _ = manager::stop(&a);
                    a.exit(0);
                });
            }
            _ => {}
        })
        .build(app)?;

    {
        let state = app.state::<AppState>();
        state.tray.lock().unwrap().replace(tray);
        state
            .tray_status_item
            .lock()
            .unwrap()
            .replace(status_item);
        state.tray_items.lock().unwrap().replace(TrayItems {
            start: start_i,
            stop: stop_i,
            restart: restart_i,
            open: open_i,
            open_web: open_web_i,
            quit: quit_i,
        });
    }
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn update(app: &AppHandle) {
    let status = app.state::<AppState>().manager.lock().unwrap().status;
    let lang = current_lang(app);
    let text = tray_text(&lang);
    let label = status_label(status, &lang);
    let state = app.state::<AppState>();
    {
        let tray_guard = state.tray.lock().unwrap();
        if let Some(tray) = tray_guard.as_ref() {
            let _ = tray.set_tooltip(Some(format!("DSH-start · DSH: {label}")));
        }
    }
    {
        let status_guard = state.tray_status_item.lock().unwrap();
        if let Some(item) = status_guard.as_ref() {
            let _ = item.set_text(format!("{}{label}", text.status_prefix));
        }
    }
    // 生命周期操作仅对托管实例有意义；外部实例只能看、不能管
    let (can_start, can_stop, can_restart, can_open) = match status {
        DshStatus::Running | DshStatus::Starting => (false, true, true, true),
        DshStatus::ExternalRunning => (false, false, false, true),
        DshStatus::Installing | DshStatus::NodeMissing => (false, false, false, false),
        _ => (true, false, false, false),
    };
    let items_guard = state.tray_items.lock().unwrap();
    if let Some(items) = items_guard.as_ref() {
        // 每次 update 顺带刷文本，语言切换即时生效
        let _ = items.start.set_text(text.start);
        let _ = items.stop.set_text(text.stop);
        let _ = items.restart.set_text(text.restart);
        let _ = items.open.set_text(text.open);
        let _ = items.open_web.set_text(text.open_web);
        let _ = items.quit.set_text(text.quit);
        let _ = items.start.set_enabled(can_start);
        let _ = items.stop.set_enabled(can_stop);
        let _ = items.restart.set_enabled(can_restart);
        let _ = items.open_web.set_enabled(can_open);
    }
}
