#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use tauri::Manager;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "service" {
        if let Err(e) = singboard_lib::service::wrapper::run_service() {
            eprintln!("Service error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    run_gui();
}

fn run_gui() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 已有实例运行时，聚焦到已有窗口
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
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
            singboard_lib::commands::binary::get_singbox_version,
            singboard_lib::commands::srs::srs_match,
            singboard_lib::commands::srs::srs_match_provider,
            singboard_lib::commands::network::fetch_url,
            singboard_lib::commands::network::http_ping,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                std::process::exit(0);
            }
        });
}
