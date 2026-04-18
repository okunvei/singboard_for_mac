use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState,
    ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

use super::scm::read_service_params;
use serde_json::Value;

// 静态变量，存储服务名称
static SERVICE_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

// 启动日志记录的时长（秒）
const STARTUP_LOG_SECONDS: u64 = 10;

fn get_service_name() -> &'static str {
    SERVICE_NAME.get().map(|s| s.as_str()).unwrap_or("sing-box")
}

pub fn run_service(service_name: &str) -> Result<(), String> {
    SERVICE_NAME.set(service_name.to_string()).ok();
    service_dispatcher::start(get_service_name(), ffi_service_main)
        .map_err(|e| format!("无法启动服务调度器: {:?}", e))
}

windows_service::define_windows_service!(ffi_service_main, service_main);

fn service_main(arguments: Vec<OsString>) {
    if let Err(e) = run_service_inner(arguments) {
        eprintln!("服务运行出错: {}", e);
    }
}

fn make_stopped_status() -> ServiceStatus {
    ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(1),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

fn run_service_inner(_arguments: Vec<OsString>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let svc_name = get_service_name();
    let status_handle = service_control_handler::register(svc_name, event_handler)
        .map_err(|e| format!("注册服务控制处理器失败: {:?}", e))?;

    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    }).ok();

    let (singbox_path, config_path, working_dir) = read_service_params(svc_name)?;

    // 处理配置：如果不符合“用户自定义日志”条件，则强制开启 Trace 模式
    let (runtime_config, use_user_log) = process_config_logic(&config_path, svc_name)?;

    let log_path = PathBuf::from(&config_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(get_appdata_dir)
        .join("startup.log");
    let _ = fs::remove_file(&log_path);

    let mut child = spawn_singbox(&singbox_path, &runtime_config, &working_dir, use_user_log)?;

    if !use_user_log {
        pump_logs(&mut child, log_path);
    }

    std::thread::sleep(Duration::from_secs(2));
    if let Ok(Some(_)) = child.try_wait() {
        status_handle.set_service_status(make_stopped_status()).ok();
        return Err("sing-box 启动失败，请检查 startup.log".into());
    }

    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }).ok();

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(()) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    status_handle.set_service_status(make_stopped_status()).ok();

    Ok(())
}

/// 逻辑核心：判断是否需要强制开启 Trace 模式
fn process_config_logic(config_path: &str, svc_name: &str) -> Result<(String, bool), String> {
    let content = fs::read_to_string(config_path).map_err(|e| format!("读取配置失败: {}", e))?;
    let mut json: Value = serde_json::from_str(&content).map_err(|e| format!("解析 JSON 失败: {}", e))?;

    // 获取日志配置节点
    let log_node = json.get("log");
    let disabled = log_node.and_then(|l| l.get("disabled")).and_then(|v| v.as_bool()).unwrap_or(false);
    let output = log_node.and_then(|l| l.get("output")).and_then(|v| v.as_str()).unwrap_or("");
    
    // 判断是否满足用户自定义且有效的日志输出条件
    let use_user_log = !disabled && !output.trim().is_empty();

    if use_user_log {
        // 满足条件：直接使用用户原有的配置文件
        Ok((config_path.to_string(), true))
    } else {
        // 不满足条件：强制开启 Trace 模式并生成临时配置
        if let Some(obj) = json.get_mut("log").and_then(|l| l.as_object_mut()) {
            // 🎯 这里就是你要求的逻辑：强制设为 trace
            obj.insert("level".to_string(), Value::String("trace".to_string()));
            obj.insert("disabled".to_string(), Value::Bool(false));
            obj.insert("output".to_string(), Value::String("".to_string()));
        } else {
            // 如果原本没有 log 部分，直接补齐一个 Trace 级别的 log 节点
            json["log"] = serde_json::json!({
                "level": "trace",
                "disabled": false,
                "timestamp": true,
                "output": ""
            });
        }
        
        let mut temp_path = std::env::temp_dir();
        temp_path.push(format!("{}_runtime.json", svc_name));
        
        fs::write(&temp_path, serde_json::to_string_pretty(&json).unwrap())
            .map_err(|e| format!("写入临时配置失败: {}", e))?;
            
        Ok((temp_path.to_string_lossy().to_string(), false))
    }
}

fn spawn_singbox(
    singbox_path: &str,
    runtime_config: &str,
    working_dir: &str,
    use_user_log: bool,
) -> Result<Child, String> {
    let work_dir = if working_dir.is_empty() {
        PathBuf::from(runtime_config).parent().unwrap_or(&PathBuf::from(".")).to_path_buf()
    } else {
        PathBuf::from(working_dir)
    };

    let mut cmd = Command::new(singbox_path);
    cmd.args(["run", "-c", runtime_config, "-D", &work_dir.to_string_lossy()]);
    cmd.current_dir(&work_dir);

    if use_user_log {
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    } else {
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    cmd.spawn().map_err(|e| format!("进程启动异常: {}", e))
}

fn pump_logs(child: &mut Child, log_path: PathBuf) {
    let start_time = Instant::now();

    if let Some(stdout) = child.stdout.take() {
        let path = log_path.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut file = File::create(path).ok();
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok() {
                if line.is_empty() { break; }
                if start_time.elapsed().as_secs() < STARTUP_LOG_SECONDS {
                    if let Some(f) = file.as_mut() {
                        let _ = f.write_all(line.as_bytes());
                        let _ = f.flush();
                    }
                } else {
                    file = None; 
                }
                line.clear();
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let path = log_path.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut file = File::options().append(true).create(true).open(path).ok();
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok() {
                if line.is_empty() { break; }
                if start_time.elapsed().as_secs() < STARTUP_LOG_SECONDS {
                    if let Some(f) = file.as_mut() {
                        let _ = f.write_all(line.as_bytes());
                        let _ = f.flush();
                    }
                } else {
                    file = None;
                }
                line.clear();
            }
        });
    }
}

fn get_appdata_dir() -> PathBuf {
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let mut path = PathBuf::from(userprofile);
        path.push("AppData");
        path.push("Roaming");
        path.push("singboard");
        return path;
    }
    PathBuf::from(".")
}