use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{ERROR_SERVICE_DOES_NOT_EXIST, GetLastError};
use windows_sys::Win32::System::Services::*;

type ScHandle = *mut std::ffi::c_void;
pub const SERVICE_ERROR_LOG_NAME: &str = "singbox_last_error.log";

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn is_log_dir_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("log") || name.eq_ignore_ascii_case("logs")
}

fn find_log_dir(base_dir: &Path) -> Option<PathBuf> {
    for candidate in ["log", "logs"] {
        let path = base_dir.join(candidate);
        if path.is_dir() {
            return Some(path);
        }
    }

    let mut stack = vec![base_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_log_dir_name(name) {
                    return Some(path);
                }
            }
            stack.push(path);
        }
    }

    None
}

fn resolve_service_base_dir(service_name: &str) -> Option<PathBuf> {
    let (singbox_path, config_path, working_dir) = read_service_params(service_name).ok()?;

    if !working_dir.trim().is_empty() {
        let path = PathBuf::from(working_dir.trim());
        if path.is_dir() {
            return Some(path);
        }
    }

    let config = Path::new(config_path.trim());
    if let Some(parent) = config.parent() {
        if parent.is_dir() {
            return Some(parent.to_path_buf());
        }
    }

    let singbox = Path::new(singbox_path.trim());
    if let Some(parent) = singbox.parent() {
        if parent.is_dir() {
            return Some(parent.to_path_buf());
        }
    }

    None
}

pub fn resolve_service_error_log_path(service_name: &str) -> PathBuf {
    let base_dir = resolve_service_base_dir(service_name)
        .or_else(|| std::env::current_exe().ok().and_then(|exe| exe.parent().map(|p| p.to_path_buf())))
        .unwrap_or_default();

    let log_dir = find_log_dir(&base_dir).unwrap_or(base_dir);
    log_dir.join(SERVICE_ERROR_LOG_NAME)
}

fn to_wide_multi(strings: &[&str]) -> Vec<u16> {
    let mut result = Vec::new();
    for s in strings {
        result.extend(OsStr::new(s).encode_wide());
        result.push(0);
    }
    result.push(0);
    result
}

fn open_scm() -> Result<ScHandle, String> {
    unsafe {
        let handle = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_ALL_ACCESS);
        if handle.is_null() {
            Err(format!("Failed to open SCM: error {}", GetLastError()))
        } else {
            Ok(handle)
        }
    }
}

fn open_service_handle(scm: ScHandle, name: &str, access: u32) -> Result<ScHandle, String> {
    let wide_name = to_wide(name);
    unsafe {
        let handle = OpenServiceW(scm, wide_name.as_ptr(), access);
        if handle.is_null() {
            let err = GetLastError();
            if err == ERROR_SERVICE_DOES_NOT_EXIST {
                Err("service_not_found".into())
            } else {
                Err(format!("Failed to open service: error {}", err))
            }
        } else {
            Ok(handle)
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct ServiceStatus {
    pub state: String,
    pub pid: Option<u32>,
}

pub fn query_service_status(service_name: &str) -> Result<ServiceStatus, String> {
    unsafe {
        let scm = open_scm()?;
        let svc = match open_service_handle(scm, service_name, SERVICE_QUERY_STATUS) {
            Ok(h) => h,
            Err(e) if e == "service_not_found" => {
                CloseServiceHandle(scm);
                return Ok(ServiceStatus {
                    state: "not_installed".into(),
                    pid: None,
                });
            }
            Err(e) => {
                CloseServiceHandle(scm);
                return Err(e);
            }
        };

        let mut status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
        let mut bytes_needed: u32 = 0;
        let ok = QueryServiceStatusEx(
            svc,
            SC_STATUS_PROCESS_INFO,
            &mut status as *mut _ as *mut u8,
            std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut bytes_needed,
        );

        CloseServiceHandle(svc);
        CloseServiceHandle(scm);

        if ok == 0 {
            return Err(format!("QueryServiceStatusEx failed: error {}", GetLastError()));
        }

        let state = match status.dwCurrentState {
            SERVICE_RUNNING => "running",
            SERVICE_STOPPED => "stopped",
            SERVICE_START_PENDING => "starting",
            SERVICE_STOP_PENDING => "stopping",
            SERVICE_PAUSE_PENDING => "stopping",
            SERVICE_PAUSED => "stopped",
            SERVICE_CONTINUE_PENDING => "starting",
            _ => "unknown",
        };

        Ok(ServiceStatus {
            state: state.into(),
            pid: if status.dwProcessId != 0 {
                Some(status.dwProcessId)
            } else {
                None
            },
        })
    }
}

pub fn start_service(service_name: &str) -> Result<(), String> {
    unsafe {
        let scm = open_scm()?;
        let svc = open_service_handle(scm, service_name, SERVICE_START | SERVICE_QUERY_STATUS)?;

        let ok = StartServiceW(svc, 0, ptr::null());

        if ok == 0 {
            let err = GetLastError();
            CloseServiceHandle(svc);
            CloseServiceHandle(scm);
            if err == 1056 {
                return Ok(());
            }
            return Err(format!("StartService failed: error {}", err));
        }

        // Wait for the service to leave START_PENDING and verify it's running
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(250));
            let mut status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
            let mut bytes_needed: u32 = 0;
            let qok = QueryServiceStatusEx(
                svc,
                SC_STATUS_PROCESS_INFO,
                &mut status as *mut _ as *mut u8,
                std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut bytes_needed,
            );
            if qok == 0 {
                continue;
            }
            match status.dwCurrentState {
                SERVICE_RUNNING => {
                    CloseServiceHandle(svc);
                    CloseServiceHandle(scm);
                    return Ok(());
                }
                SERVICE_STOPPED => {
                    CloseServiceHandle(svc);
                    CloseServiceHandle(scm);
                    // 读取错误日志获取具体报错
                    let detail = read_service_error_log(service_name)
                        .unwrap_or_default();
                    let msg = if detail.is_empty() {
                        "服务启动后立即退出，可能是配置文件有误，请检查配置".to_string()
                    } else {
                        format!("服务启动失败:\n{}", detail)
                    };
                    return Err(msg);
                }
                _ => continue,
            }
        }

        CloseServiceHandle(svc);
        CloseServiceHandle(scm);
        Ok(())
    }
}

pub fn stop_service(service_name: &str) -> Result<(), String> {
    unsafe {
        let scm = open_scm()?;
        let svc = open_service_handle(scm, service_name, SERVICE_STOP | SERVICE_QUERY_STATUS)?;

        let mut status: SERVICE_STATUS = std::mem::zeroed();
        let ok = ControlService(svc, SERVICE_CONTROL_STOP, &mut status);

        if ok == 0 {
            let err = GetLastError();
            CloseServiceHandle(svc);
            CloseServiceHandle(scm);
            if err == 1062 {
                return Ok(());
            }
            return Err(format!("ControlService(STOP) failed: error {}", err));
        }

        for _ in 0..30 {
            thread::sleep(Duration::from_millis(500));
            let mut bytes_needed: u32 = 0;
            let mut proc_status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
            QueryServiceStatusEx(
                svc,
                SC_STATUS_PROCESS_INFO,
                &mut proc_status as *mut _ as *mut u8,
                std::mem::size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut bytes_needed,
            );
            if proc_status.dwCurrentState == SERVICE_STOPPED {
                break;
            }
        }

        CloseServiceHandle(svc);
        CloseServiceHandle(scm);
        Ok(())
    }
}

pub fn restart_service(service_name: &str) -> Result<(), String> {
    stop_service(service_name)?;
    thread::sleep(Duration::from_millis(500));
    start_service(service_name)
}

pub fn install_service(
    service_name: &str,
    bin_path: &str,
    display_name: &str,
) -> Result<(), String> {
    let wide_name = to_wide(service_name);
    let wide_display = to_wide(display_name);
    let wide_bin = to_wide(bin_path);
    let dependency_multi = to_wide_multi(&["Tcpip", "NlaSvc"]);

    unsafe {
        let scm = open_scm()?;

        let svc = CreateServiceW(
            scm,
            wide_name.as_ptr(),
            wide_display.as_ptr(),
            SERVICE_ALL_ACCESS,
            SERVICE_WIN32_OWN_PROCESS,
            SERVICE_AUTO_START,
            SERVICE_ERROR_NORMAL,
            wide_bin.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            dependency_multi.as_ptr(),
            ptr::null(),
            ptr::null(),
        );

        CloseServiceHandle(scm);

        if svc.is_null() {
            let err = GetLastError();
            if err == 1073 {
                return Ok(());
            }
            return Err(format!("CreateService failed: error {}", err));
        }

        let desc_text = to_wide("Proxy service managed by singboard");
        let mut desc = SERVICE_DESCRIPTIONW {
            lpDescription: desc_text.as_ptr() as *mut _,
        };
        ChangeServiceConfig2W(
            svc,
            SERVICE_CONFIG_DESCRIPTION,
            &mut desc as *mut _ as *mut _,
        );

        let mut actions = [
            SC_ACTION {
                Type: SC_ACTION_RESTART,
                Delay: 5000,
            },
            SC_ACTION {
                Type: SC_ACTION_RESTART,
                Delay: 10000,
            },
            SC_ACTION {
                Type: SC_ACTION_NONE,
                Delay: 0,
            },
        ];
        let mut failure = SERVICE_FAILURE_ACTIONSW {
            dwResetPeriod: 86400,
            lpRebootMsg: ptr::null_mut(),
            lpCommand: ptr::null_mut(),
            cActions: 3,
            lpsaActions: actions.as_mut_ptr(),
        };
        ChangeServiceConfig2W(
            svc,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            &mut failure as *mut _ as *mut _,
        );

        CloseServiceHandle(svc);
        Ok(())
    }
}

pub fn uninstall_service(service_name: &str) -> Result<(), String> {
    let _ = stop_service(service_name);

    unsafe {
        let scm = open_scm()?;
        let svc = open_service_handle(scm, service_name, SERVICE_ALL_ACCESS)?;

        let ok = DeleteService(svc);

        CloseServiceHandle(svc);
        CloseServiceHandle(scm);

        if ok == 0 {
            let err = GetLastError();
            if err == 1072 {
                return Ok(());
            }
            return Err(format!("DeleteService failed: error {}", err));
        }
        Ok(())
    }
}

pub fn write_service_params(
    service_name: &str,
    singbox_path: &str,
    config_path: &str,
    working_dir: &str,
) -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key_path = format!(
        "SYSTEM\\CurrentControlSet\\Services\\{}\\Parameters",
        service_name
    );
    let (key, _) = hklm
        .create_subkey(&key_path)
        .map_err(|e| format!("Failed to create registry key: {}", e))?;
    key.set_value("SingboxPath", &singbox_path)
        .map_err(|e| format!("Failed to write SingboxPath: {}", e))?;
    key.set_value("ConfigPath", &config_path)
        .map_err(|e| format!("Failed to write ConfigPath: {}", e))?;
    key.set_value("WorkingDir", &working_dir)
        .map_err(|e| format!("Failed to write WorkingDir: {}", e))?;
    Ok(())
}

pub fn read_service_params(service_name: &str) -> Result<(String, String, String), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key_path = format!(
        "SYSTEM\\CurrentControlSet\\Services\\{}\\Parameters",
        service_name
    );
    let key = hklm
        .open_subkey(&key_path)
        .map_err(|e| format!("Failed to open registry key: {}", e))?;
    let singbox_path: String = key
        .get_value("SingboxPath")
        .map_err(|e| format!("Failed to read SingboxPath: {}", e))?;
    let config_path: String = key
        .get_value("ConfigPath")
        .map_err(|e| format!("Failed to read ConfigPath: {}", e))?;
    let working_dir: String = key
        .get_value("WorkingDir")
        .unwrap_or_default();
    Ok((singbox_path, config_path, working_dir))
}

/// 读取 sing-box 最近一次的错误日志
pub fn read_service_error_log(service_name: &str) -> Result<String, String> {
    let log_path = resolve_service_error_log_path(service_name);
    std::fs::read_to_string(&log_path)
        .map_err(|e| format!("Failed to read error log: {}", e))
}
