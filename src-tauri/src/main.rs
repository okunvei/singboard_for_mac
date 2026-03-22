#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use tauri::Manager;

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

fn run_gui() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 已有实例运行时，聚焦到已有窗口
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::new()
            .with_state_flags(
                tauri_plugin_window_state::StateFlags::all()
                    .difference(tauri_plugin_window_state::StateFlags::VISIBLE),
            )
            .build())
        .plugin(tauri_plugin_dialog::init())
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
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            use tauri_plugin_window_state::AppHandleExt;
            let state_flags = tauri_plugin_window_state::StateFlags::all()
                .difference(tauri_plugin_window_state::StateFlags::VISIBLE);
            match event {
                tauri::RunEvent::WindowEvent {
                    event:
                        tauri::WindowEvent::Resized(_)
                        | tauri::WindowEvent::Moved(_),
                    ..
                } => {
                    let _ = app.save_window_state(state_flags);
                }
                tauri::RunEvent::ExitRequested { .. } => {
                    std::process::exit(0);
                }
                _ => {}
            }
        });
}
