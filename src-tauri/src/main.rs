#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;
use tauri::Manager;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn set_close_to_tray(enabled: bool) {
    CLOSE_TO_TRAY.store(enabled, Ordering::Relaxed);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "service" {
        let service_name = args.get(2).cloned().unwrap_or_else(|| "sing-box".to_string());
        if let Err(e) = singboard_lib::service::wrapper::run_service(&service_name) {
            eprintln!("Service error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    run_gui();
}

fn show_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.emit("window-visibility", true);
    }
}

fn run_gui() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::new()
            .with_state_flags(
                tauri_plugin_window_state::StateFlags::all()
                    .difference(tauri_plugin_window_state::StateFlags::VISIBLE),
            )
            .build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init()) // ✨ 新增
        .setup(|app| {
            let app_handle = app.handle().clone();

            let show = MenuItemBuilder::with_id("show", "打开面板")
                .build(&app_handle).expect("menu item");
            let sep = PredefinedMenuItem::separator(&app_handle).expect("separator");
            let quit = MenuItemBuilder::with_id("quit", "退出")
                .build(&app_handle).expect("menu item");
            let menu = MenuBuilder::new(&app_handle)
                .item(&show)
                .item(&sep)
                .item(&quit)
                .build()
                .expect("tray menu");

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().cloned().expect("app icon"))
                .tooltip("Singboard")
                .menu(&menu)
                // --- 显式禁用左键点击弹出菜单 ---
                .menu_on_left_click(false) 
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::Click { button, button_state, .. } = event {
                        // 建议使用 MouseButtonState::Down 或 Up 的其中一个
                        if button == MouseButton::Left && button_state == MouseButtonState::Up {
                            let handle = tray.app_handle();
                            // --- 使用 async_runtime 稍微解耦，避免阻塞当前事件循环导致的 UI 冲突 ---
                            let handle_clone = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                show_window(&handle_clone);
                            });
                        }
                    }
                })
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => show_window(app),
                        "quit" => app.exit(0),
                        _ => {}
                    }
                })
                .build(&app.handle().clone())
                .expect("tray icon");

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    if CLOSE_TO_TRAY.load(Ordering::Relaxed) {
                        api.prevent_close();
                        let _ = window.emit("window-visibility", false);
                        let _ = window.hide();
                    } else {
                        let _ = window.app_handle().exit(0);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            set_close_to_tray,
            singboard_lib::commands::service::service_status,
            singboard_lib::commands::service::service_start,
            singboard_lib::commands::service::service_stop,
            singboard_lib::commands::service::service_restart,
            singboard_lib::commands::service::service_install,
            singboard_lib::commands::service::service_uninstall,
            singboard_lib::commands::service::service_error_log,
            singboard_lib::commands::config::read_config,
            singboard_lib::commands::config::write_config,
            singboard_lib::commands::config::validate_config,
            singboard_lib::commands::config::validate_config_content,
            singboard_lib::commands::config::detect_runtime_files,
            singboard_lib::commands::config::get_running_config_path,
            singboard_lib::commands::config::copy_to_running_config,
            singboard_lib::commands::config::get_remote_config_dir,
            singboard_lib::commands::config::get_remote_config_path,
            singboard_lib::commands::config::delete_file,
            singboard_lib::commands::binary::get_singbox_version,
            singboard_lib::commands::srs::srs_match,
            singboard_lib::commands::srs::srs_match_provider,
            singboard_lib::commands::srs::srs_list_provider,
            singboard_lib::commands::network::fetch_url,
            singboard_lib::commands::network::http_ping,
            singboard_lib::commands::network::set_self_proxy,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            use tauri_plugin_window_state::AppHandleExt;
            let state_flags = tauri_plugin_window_state::StateFlags::all()
                .difference(tauri_plugin_window_state::StateFlags::VISIBLE);
            match event {
                tauri::RunEvent::WindowEvent {
                    label,
                    event:
                        tauri::WindowEvent::Resized(_)
                        | tauri::WindowEvent::Moved(_),
                    ..
                } => {
                    if label == "main" {
                        let _ = app.save_window_state(state_flags);
                    }
                }
                tauri::RunEvent::ExitRequested { .. } => {}
                _ => {}
            }
        });
}
