//! scm.rs — macOS 服务管理层
//!
//! 架构：一个特权 Helper（root 常驻）负责所有需要 root 的操作。
//! 主 App 通过 Unix Socket 与 Helper 通信，无需反复输入密码。
//!
//! 用户视角：
//!   - 「安装服务」= 自动装 Helper（弹一次密码）+ 装 sing-box 服务
//!   - 「启动/停止/重启」= 完全无感，不弹密码
//!   - 「卸载服务」= 停止服务 + 卸载 Helper（弹一次密码）

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

pub const SERVICE_ERROR_LOG_NAME: &str = "singbox_last_error.log";
const SOCKET_PATH: &str = "/var/run/singboard-helper.sock";
const TOKEN_PATH: &str = "/var/run/singboard-helper.token";

/// 读取 Helper 写入的 token，用于鉴权
fn read_helper_token() -> String {
    std::fs::read_to_string(TOKEN_PATH)
        .unwrap_or_default()
        .trim()
        .to_string()
}

const HELPER_LABEL: &str = "com.singboard.helper";
const HELPER_PLIST_PATH: &str = "/Library/LaunchDaemons/com.singboard.helper.plist";

// ── Helper 通信 ──────────────────────────────────────────────────────────────

/// 获取 running-config.json 的路径（用于 Helper 解析 TUN 配置）
fn get_running_config_path_internal() -> String {
    // running-config.json 存放在 ~/Library/Application Support/singboard/running-config.json
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join("Library/Application Support/singboard/running-config.json")
        .to_string_lossy()
        .to_string()
}

/// 检查 Helper 是否真正在运行（发一个 ping 确认）
pub fn is_helper_running() -> bool {
    // 尝试发送 ping
    match send_to_helper(serde_json::json!({"cmd": "ping"})) {
        Ok(resp) => resp["ok"].as_bool().unwrap_or(false),
        Err(e) => {
            // 如果日志里出现 "Permission denied"，说明 Helper 进程已在监听，但 chown 还没完成
            // 此时返回 false 是正确的，这会让外层的 install 循环继续重试直到权限正常
            eprintln!("[scm] Helper 连接尝试中: {}", e);
            false
        }
    }
}

/// 向 Helper 发送 JSON 指令，返回响应
fn send_to_helper(req: serde_json::Value) -> Result<serde_json::Value, String> {
    let stream = UnixStream::connect(SOCKET_PATH)
        .map_err(|e| format!("无法连接到 Helper: {}", e))?;

    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    // ping 指令不需要 token，其他指令自动附加
    let req = if req["cmd"].as_str() != Some("ping") {
        let mut r = req.as_object().cloned().unwrap_or_default();
        r.insert("token".to_string(), serde_json::Value::String(read_helper_token()));
        serde_json::Value::Object(r)
    } else {
        req
    };

    let mut stream_w = stream.try_clone().map_err(|e| e.to_string())?;
    let mut req_str = req.to_string();
    req_str.push('\n');
    stream_w.write_all(req_str.as_bytes())
        .map_err(|e| format!("发送指令失败: {}", e))?;

    let reader = BufReader::new(stream);
    let line = reader.lines().next()
        .ok_or_else(|| "Helper 无响应".to_string())?
        .map_err(|e| format!("读取响应失败: {}", e))?;

    let resp: serde_json::Value =
        serde_json::from_str(&line).map_err(|e| format!("解析响应失败: {}", e))?;

    if resp["ok"].as_bool().unwrap_or(false) {
        Ok(resp)
    } else {
        Err(resp["error"].as_str().unwrap_or("未知错误").to_string())
    }
}

// ── osascript 提权（仅用于安装/卸载 Helper 本身） ───────────────────────────

fn run_privileged(shell_cmd: &str) -> Result<(), String> {
    // 对 shell_cmd 中的特殊字符转义（双引号、反斜杠）
    let escaped = shell_cmd.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("do shell script \"{}\" with administrator privileges", escaped);
    let out = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript 执行失败: {}", e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let msg = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("管理员权限操作失败: {}", msg));
    }
    Ok(())
}

// ── 参数持久化 ───────────────────────────────────────────────────────────────

fn params_dir(service_name: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join("Library/Application Support/singboard")
        .join(service_name)
}

fn params_file(service_name: &str) -> PathBuf {
    params_dir(service_name).join("params.json")
}

pub fn write_service_params(
    service_name: &str,
    singbox_path: &str,
    config_path: &str,
    working_dir: &str,
) -> Result<(), String> {
    let dir = params_dir(service_name);
    fs::create_dir_all(&dir).map_err(|e| format!("创建参数目录失败: {}", e))?;
    let json = serde_json::json!({
        "singbox_path": singbox_path,
        "config_path":  config_path,
        "working_dir":  working_dir,
    });
    fs::write(params_file(service_name), json.to_string())
        .map_err(|e| format!("写入参数失败: {}", e))
}

pub fn read_service_params(service_name: &str) -> Result<(String, String, String), String> {
    let content = fs::read_to_string(params_file(service_name))
        .map_err(|e| format!("读取参数失败: {}", e))?;
    let v: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析参数失败: {}", e))?;
    Ok((
        v["singbox_path"].as_str().unwrap_or("").to_string(),
        v["config_path"].as_str().unwrap_or("").to_string(),
        v["working_dir"].as_str().unwrap_or("").to_string(),
    ))
}

// ── 日志路径辅助 ─────────────────────────────────────────────────────────────

fn is_log_dir_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("log") || name.eq_ignore_ascii_case("logs")
}

fn find_log_dir(base_dir: &Path) -> Option<PathBuf> {
    for candidate in ["log", "logs"] {
        let path = base_dir.join(candidate);
        if path.is_dir() { return Some(path); }
    }
    let mut stack = vec![base_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) { Ok(e) => e, Err(_) => continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_log_dir_name(name) { return Some(path); }
            }
            stack.push(path);
        }
    }
    None
}

fn resolve_service_base_dir(service_name: &str) -> Option<PathBuf> {
    let (singbox_path, config_path, working_dir) = read_service_params(service_name).ok()?;
    if !working_dir.trim().is_empty() {
        let p = PathBuf::from(working_dir.trim());
        if p.is_dir() { return Some(p); }
    }
    let config = Path::new(config_path.trim());
    if let Some(parent) = config.parent() {
        if parent.is_dir() { return Some(parent.to_path_buf()); }
    }
    let singbox = Path::new(singbox_path.trim());
    if let Some(parent) = singbox.parent() {
        if parent.is_dir() { return Some(parent.to_path_buf()); }
    }
    None
}

pub fn resolve_service_error_log_path(service_name: &str) -> PathBuf {
    let base_dir = resolve_service_base_dir(service_name)
        .or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())))
        .unwrap_or_default();
    let log_dir = find_log_dir(&base_dir).unwrap_or(base_dir);
    log_dir.join(SERVICE_ERROR_LOG_NAME)
}

// ── Helper 自身安装/卸载 ─────────────────────────────────────────────────────

/// 安装 Helper（需要管理员密码，只调用一次）
fn install_helper_if_needed() -> Result<(), String> {
    // 如果 Helper 已经在运行，直接返回
    if is_helper_running() {
        return Ok(());
    }

    let helper_exe = std::env::current_exe()
        .map_err(|e| format!("获取程序路径失败: {}", e))?
        .parent()
        .ok_or("无法获取程序目录")?
        .join("singboard-helper")
        .to_string_lossy()
        .to_string();

    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\"\n\
  \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
    <key>Label</key>\n\
    <string>{label}</string>\n\
    <key>ProgramArguments</key>\n\
    <array>\n\
        <string>{exe}</string>\n\
    </array>\n\
    <key>RunAtLoad</key>\n\
    <true/>\n\
    <key>KeepAlive</key>\n\
    <true/>\n\
    <key>StandardErrorPath</key>\n\
    <string>/var/log/singboard-helper.log</string>\n\
    <key>EnvironmentVariables</key>\n\
    <dict>\n\
        <key>SINGBOARD_CALLER_UID</key>\n\
        <string>{uid}</string>\n\
    </dict>\n\
</dict>\n\
</plist>\n",
        label = HELPER_LABEL,
        exe = helper_exe,
        uid = unsafe { libc::getuid() },
    );

    let tmp = std::env::temp_dir().join("singboard-helper.plist.tmp");
    fs::write(&tmp, &plist).map_err(|e| format!("写入临时 plist 失败: {}", e))?;

    let tmp_str = tmp.to_string_lossy().to_string();

    // 检查是否已安装过（plist 存在但 Helper 没跑）——先 bootout 再重装
    let plist_exists = PathBuf::from(HELPER_PLIST_PATH).exists();

    let shell_cmd = if plist_exists {
        format!(
            "launchctl bootout system/{label} 2>/dev/null; \
             cp '{tmp}' '{dest}' && \
             chown root:wheel '{dest}' && \
             chmod 644 '{dest}' && \
             chmod +x '{exe}' && \
             launchctl bootstrap system '{dest}'",
            label = HELPER_LABEL,
            tmp = tmp_str,
            dest = HELPER_PLIST_PATH,
            exe = helper_exe,
        )
    } else {
        format!(
            "cp '{tmp}' '{dest}' && \
             chown root:wheel '{dest}' && \
             chmod 644 '{dest}' && \
             chmod +x '{exe}' && \
             launchctl bootstrap system '{dest}'",
            tmp = tmp_str,
            dest = HELPER_PLIST_PATH,
            exe = helper_exe,
        )
    };

    let result = run_privileged(&shell_cmd);
    let _ = fs::remove_file(&tmp);
    result?;

    // 等待 Helper 启动（最多 5 秒）
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(250));
        if is_helper_running() {
            return Ok(());
        }
    }

    Err("Helper 启动超时，请重试".to_string())
}

/// 卸载 Helper（需要管理员密码）
pub fn uninstall_helper() -> Result<(), String> {
    let shell_cmd = format!(
        "launchctl bootout system/{label} 2>/dev/null; rm -f '{plist}'",
        label = HELPER_LABEL,
        plist = HELPER_PLIST_PATH,
    );
    run_privileged(&shell_cmd)
}

// ── 公共服务接口 ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct ServiceStatus {
    pub state: String,
    pub pid: Option<u32>,
}

pub fn query_service_status(service_name: &str) -> Result<ServiceStatus, String> {
    if !is_helper_running() {
        // Helper 未运行：通过 plist 文件判断是否已安装
        let plist = PathBuf::from("/Library/LaunchDaemons")
            .join(format!("com.singboard.{}.plist", service_name));
        return Ok(ServiceStatus {
            state: if plist.exists() { "stopped".into() } else { "not_installed".into() },
            pid: None,
        });
    }

    let resp = send_to_helper(serde_json::json!({
        "cmd": "status",
        "service": service_name,
    }))?;

    Ok(ServiceStatus {
        state: resp["state"].as_str().unwrap_or("stopped").to_string(),
        pid: resp["pid"].as_u64().map(|v| v as u32),
    })
}

pub fn start_service(service_name: &str) -> Result<(), String> {
    if !is_helper_running() {
        return Err("Helper 未运行，请先安装服务".to_string());
    }
    // 把 running-config 路径传给 Helper，用于解析 TUN 配置自动设置 DNS
    let config_path = get_running_config_path_internal();
    send_to_helper(serde_json::json!({
        "cmd": "start",
        "service": service_name,
        "config_path": config_path,
    }))?;
    Ok(())
}

pub fn stop_service(service_name: &str) -> Result<(), String> {
    if !is_helper_running() { return Ok(()); }
    send_to_helper(serde_json::json!({
        "cmd": "stop",
        "service": service_name,
    }))?;
    Ok(())
}

pub fn restart_service(service_name: &str) -> Result<(), String> {
    if !is_helper_running() {
        return Err("Helper 未运行，请先安装服务".to_string());
    }
    let config_path = get_running_config_path_internal();
    send_to_helper(serde_json::json!({
        "cmd": "restart",
        "service": service_name,
        "config_path": config_path,
    }))?;
    Ok(())
}

/// 安装服务（自动安装 Helper + 安装 sing-box 服务，只弹一次密码）
pub fn install_service(
    service_name: &str,
    _bin_path: &str,
    _display_name: &str,
) -> Result<(), String> {
    // 步骤1：确保 Helper 已安装并运行（如需安装则弹密码框）
    install_helper_if_needed()?;

    // 步骤2：通过 Helper 安装 sing-box 服务（无需密码）
    let (singbox_path, config_path, working_dir) = read_service_params(service_name)?;

    let exe_path = std::env::current_exe()
        .map_err(|e| format!("获取程序路径失败: {}", e))?
        .to_string_lossy()
        .to_string();

    let error_log = resolve_service_error_log_path(service_name)
        .to_string_lossy()
        .to_string();

    send_to_helper(serde_json::json!({
        "cmd": "install",
        "service": service_name,
        "exe_path": exe_path,
        "singbox_path": singbox_path,
        "config_path": config_path,
        "working_dir": working_dir,
        "error_log": error_log,
    }))?;

    Ok(())
}

/// 卸载服务（卸载 sing-box 服务 + 卸载 Helper，弹一次密码）
pub fn uninstall_service(service_name: &str) -> Result<(), String> {
    // 步骤1：通过 Helper 卸载 sing-box 服务
    if is_helper_running() {
        let _ = send_to_helper(serde_json::json!({
            "cmd": "uninstall",
            "service": service_name,
        }));
    }

    // 步骤2：卸载 Helper 本身（需要密码）
    uninstall_helper()?;

    Ok(())
}

pub fn clear_system_proxy() -> Result<(), String> {
    if !is_helper_running() { return Ok(()); }
    send_to_helper(serde_json::json!({"cmd": "clear_proxy"}))?;
    Ok(())
}

pub fn read_service_error_log(service_name: &str) -> Result<String, String> {
    let log_path = resolve_service_error_log_path(service_name);
    std::fs::read_to_string(&log_path)
        .map_err(|e| format!("Failed to read error log: {}", e))
}
