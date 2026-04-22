use crate::service::scm;

#[tauri::command]
pub async fn service_status(service_name: String) -> Result<scm::ServiceStatus, String> {
    tokio::task::spawn_blocking(move || scm::query_service_status(&service_name))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn service_start(service_name: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || scm::start_service(&service_name))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn service_stop(service_name: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || scm::stop_service(&service_name))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn service_restart(service_name: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || scm::restart_service(&service_name))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn service_install(
    service_name: String,
    singbox_path: String,
    config_path: String,
    working_dir: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let exe_path = std::env::current_exe()
            .map_err(|e| format!("Failed to get current exe path: {}", e))?;
        let bin_path = format!("\"{}\" service \"{}\"", exe_path.display(), service_name);
        let display_name = format!("{} (singboard)", service_name);

        scm::write_service_params(&service_name, &singbox_path, &config_path, &working_dir)?;
        // install_service 内部会自动安装 Helper（若未安装），只弹一次密码
        scm::install_service(&service_name, &bin_path, &display_name)?;

        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn service_uninstall(service_name: String) -> Result<(), String> {
    // uninstall_service 内部会同时卸载 Helper
    tokio::task::spawn_blocking(move || scm::uninstall_service(&service_name))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn service_error_log(service_name: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || scm::read_service_error_log(&service_name))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

/// 检查 Helper 是否在运行（前端用于显示状态）
#[tauri::command]
pub async fn helper_running() -> bool {
    tokio::task::spawn_blocking(scm::is_helper_running)
        .await
        .unwrap_or(false)
}

/// 通过 Helper 清除 macOS 系统代理
#[tauri::command]
pub async fn clear_system_proxy() -> Result<(), String> {
    tokio::task::spawn_blocking(scm::clear_system_proxy)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}
