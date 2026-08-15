mod cli;
mod commands;
mod control;
mod logger;
mod manager;
mod runtime;
mod settings;
mod state;
mod tray;

use tauri::Manager;

pub fn run() {
    let builder = tauri::Builder::default()
        // single-instance first: forwards `dsh-start restart` to the running app
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let is_restart = args.iter().any(|a| a == "restart");
            if is_restart {
                let app = app.clone();
                std::thread::spawn(move || {
                    let _ = manager::restart(&app, "cli-callback");
                });
            } else {
                tray::show_main(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_opener::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::start_dsh,
            commands::stop_dsh,
            commands::restart_dsh,
            commands::force_stop_external,
            commands::ensure_runtime,
            commands::get_runtime_info,
            commands::install_node_guided,
            commands::update_dsh,
            commands::check_update,
            commands::get_settings,
            commands::save_settings,
            commands::set_autostart,
            commands::get_autostart,
            commands::get_logs,
            commands::get_callback_info,
            commands::open_log_file,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // pre-load settings + node info + log file writer
            let loaded = settings::load(&handle);
            *handle.state::<state::AppState>().settings.lock().unwrap() = loaded;
            *handle.state::<state::AppState>().node_info.lock().unwrap() =
                runtime::detect_node().ok();
            *handle.state::<state::AppState>().log.lock().unwrap() = logger::LogRing::new(&handle);
            *handle.state::<state::AppState>().shim_dir.lock().unwrap() =
                Some(cli::shim_dir(&handle));

            // global logger → app log ring
            let _ = log::set_boxed_logger(Box::new(logger::AppLogger(handle.clone())));
            log::set_max_level(log::LevelFilter::Info);
            log::info!("DSH Start 启动 (v{})", env!("CARGO_PKG_VERSION"));

            let flags = cli::flags();

            // callback shim registration
            let register_cli = handle.state::<state::AppState>().settings.lock().unwrap().register_cli;
            if register_cli {
                let h = handle.clone();
                std::thread::spawn(move || {
                    if let Err(e) = cli::register(&h) {
                        logger::log_event(&h, "warn", &format!("注册回调命令失败: {e}"));
                    }
                });
            } else {
                cli::unregister(&handle);
            }

            // localhost control endpoint (HTTP callback restart)
            let _ = control::start(&handle);

            // reflect externally-started dsh instances in the status
            manager::watch_external(handle.clone());

            // tray
            let _ = tray::build(&handle);

            // initial status push to the UI
            manager::emit_status(&handle);

            // show the console unless launched hidden
            if !flags.minimized && !flags.restart {
                tray::show_main(&handle);
            }

            // autostart enabled → bring dsh up automatically
            {
                use tauri_plugin_autostart::ManagerExt;
                if handle.autolaunch().is_enabled().unwrap_or(false) {
                    logger::log_event(&handle, "info", "开机启动已开启，自动启动 DSH…");
                    let h = handle.clone();
                    std::thread::spawn(move || {
                        let _ = manager::start(&h);
                    });
                }
            }

            // primary instance invoked as `dsh-start restart` → restart dsh now
            if flags.restart {
                logger::log_event(&handle, "info", "收到 restart 参数，执行重启…");
                let h = handle.clone();
                std::thread::spawn(move || {
                    let _ = manager::restart(&h, "cli-primary");
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // closing the console hides to tray; quit via the tray menu
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        });

    builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                let quit_stops = app_handle
                    .state::<state::AppState>()
                    .settings
                    .lock()
                    .unwrap()
                    .quit_stops_dsh;
                if quit_stops {
                    let _ = manager::stop(app_handle);
                }
            }
        });
}
