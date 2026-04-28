use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use super::scm::read_service_params;
use serde_json::Value;

// 启动日志记录时长（秒）
const STARTUP_LOG_SECONDS: u64 = 10;

/// macOS 入口：直接在当前进程里运行服务逻辑（无需 service_dispatcher）。
/// main.rs 检测到 "service" 子命令后调用此函数。
pub fn run_service(service_name: &str) -> Result<(), String> {
    // 优先从环境变量读取参数（launchd plist 注入），
    // 其次回退到持久化文件（params.json）。
    let (singbox_path, config_path, working_dir) = {
        let env_singbox = std::env::var("SINGBOARD_SINGBOX_PATH").unwrap_or_default();
        let env_config = std::env::var("SINGBOARD_CONFIG_PATH").unwrap_or_default();
        let env_workdir = std::env::var("SINGBOARD_WORKING_DIR").unwrap_or_default();

        if !env_singbox.is_empty() && !env_config.is_empty() {
            (env_singbox, env_config, env_workdir)
        } else {
            read_service_params(service_name)?
        }
    };

    let (runtime_config, use_user_log) =
        process_config_logic(&config_path, service_name)?;

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

    // 等待 2 秒确认进程没有立即崩溃
    std::thread::sleep(Duration::from_secs(2));
    if let Ok(Some(_)) = child.try_wait() {
        return Err("sing-box 启动失败，请检查 startup.log".into());
    }

    // 监听 SIGTERM / SIGINT，收到后终止子进程
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        // 安全：使用 signal_hook 通过管道通知；这里用更简单的轮询方案
        // 注册 ctrlc（包含 SIGTERM）处理
        let _ = ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
        });
    }

    loop {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        // 子进程自行退出也结束循环
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

// ── 配置处理（逻辑与 Windows 版完全一致） ───────────────────────────────────

fn process_config_logic(config_path: &str, svc_name: &str) -> Result<(String, bool), String> {
    let content = fs::read_to_string(config_path)
        .map_err(|e| format!("读取配置失败: {}", e))?;
    let mut json: Value =
        serde_json::from_str(&content).map_err(|e| format!("解析 JSON 失败: {}", e))?;

    let log_node = json.get("log");
    let disabled = log_node
        .and_then(|l| l.get("disabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let output = log_node
        .and_then(|l| l.get("output"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let use_user_log = !disabled && !output.trim().is_empty();

    if use_user_log {
        Ok((config_path.to_string(), true))
    } else {
        if let Some(obj) = json.get_mut("log").and_then(|l| l.as_object_mut()) {
            obj.insert("level".to_string(), Value::String("trace".to_string()));
            obj.insert("disabled".to_string(), Value::Bool(false));
            obj.insert("output".to_string(), Value::String("".to_string()));
        } else {
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

// ── 进程启动 ─────────────────────────────────────────────────────────────────

fn spawn_singbox(
    singbox_path: &str,
    runtime_config: &str,
    working_dir: &str,
    use_user_log: bool,
) -> Result<Child, String> {
    let work_dir = if working_dir.is_empty() {
        PathBuf::from(runtime_config)
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .to_path_buf()
    } else {
        PathBuf::from(working_dir)
    };

    let mut cmd = Command::new(singbox_path);
    cmd.args(["run", "-c", runtime_config, "-D", &work_dir.to_string_lossy()]);
    cmd.current_dir(&work_dir);

    if use_user_log {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    } else {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    }

    cmd.spawn().map_err(|e| format!("进程启动异常: {}", e))
}

// ── 日志泵（与 Windows 版逻辑完全一致） ─────────────────────────────────────

fn pump_logs(child: &mut Child, log_path: PathBuf) {
    let start_time = Instant::now();

    if let Some(stdout) = child.stdout.take() {
        let path = log_path.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut file = File::create(path).ok();
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok() {
                if line.is_empty() {
                    break;
                }
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
                if line.is_empty() {
                    break;
                }
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

// ── 路径辅助 ─────────────────────────────────────────────────────────────────

fn get_appdata_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join("Library/Application Support/singboard")
}
