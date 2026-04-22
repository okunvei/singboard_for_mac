use reqwest;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use lazy_static::lazy_static;

// 1. 全局静态变量，用于存放前端传来的 Self Proxy 端口拼接后的字符串
lazy_static! {
    static ref SELF_PROXY: Mutex<String> = Mutex::new(String::new());
}

// 2. 提供给前端调用的命令：更新全局代理变量
#[tauri::command]
pub fn set_self_proxy(proxy: String) {
    let mut p = SELF_PROXY.lock().unwrap();
    *p = proxy;
}

fn get_system_proxy() -> Option<String> {
// --- Windows 逻辑 ---
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let settings = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
            .ok()?;

        let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
        if enabled == 0 { return None; }

        let server: String = settings.get_value("ProxyServer").ok()?;
        if server.is_empty() { return None; }

        let addr = if server.contains('=') {
            server.split(';')
                .find(|s| s.starts_with("http="))
                .map(|s| s.trim_start_matches("http=").to_string())
                .unwrap_or_else(|| server.split(';').next().unwrap_or("").to_string())
        } else {
            server
        };

        if addr.is_empty() { return None; }
        return Some(if addr.contains("://") { addr } else { format!("http://{}", addr) });
    }

    // --- macOS 逻辑 ---
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let out = Command::new("scutil").args(["--proxy"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut enabled = false;
        let mut host = String::new();
        let mut port = String::new();

        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("HTTPEnable") { enabled = line.ends_with(": 1"); }
            else if line.starts_with("HTTPProxy") { host = line.split(':').nth(1)?.trim().to_string(); }
            else if line.starts_with("HTTPPort") { port = line.split(':').nth(1)?.trim().to_string(); }
        }
        if enabled && !host.is_empty() { Some(format!("http://{}:{}", host, port)) } else { None }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    None
}

fn build_client(timeout: Option<Duration>) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder();

    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }

    // 优先级 1: 检查 Self Proxy (前端传来的 socks5h://127.0.0.1:端口)
    let self_p = SELF_PROXY.lock().unwrap().clone();
    
    let proxy_to_use = if !self_p.is_empty() {
        Some(self_p)
    } else {
        // 优先级 2: 回退到系统代理
        get_system_proxy()
    };

    match proxy_to_use.and_then(|url| reqwest::Proxy::all(&url).ok()) {
        Some(proxy) => builder.proxy(proxy),
        None => builder.no_proxy(),
    }
    .build()
}

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    let client = build_client(None).map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    resp.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn http_ping(url: String, count: u32) -> Result<f64, String> {
    let client = build_client(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;

    let mut total = 0.0;
    let mut success = 0u32;

    for _ in 0..count {
        let start = Instant::now();
        if client.head(&url).send().await.is_ok() {
            total += start.elapsed().as_secs_f64() * 1000.0;
            success += 1;
        }
    }

    if success == 0 {
        return Err("timeout".to_string());
    }
    Ok(total / success as f64)
}

/// 检测 macOS 系统代理是否开启（HTTP、HTTPS、SOCKS 任意一个开启即返回 true）
#[tauri::command]
pub async fn check_system_proxy_inbound(_config_path: String) -> Result<bool, String> {
    let out = tokio::process::Command::new("scutil")
        .args(["--proxy"])
        .output()
        .await
        .map_err(|e| format!("scutil 执行失败: {}", e))?;

    let text = String::from_utf8_lossy(&out.stdout);

    for line in text.lines() {
        let line = line.trim();
        // HTTPEnable、HTTPSEnable、SOCKSEnable 任意一个为 1 即视为代理开启
        if (line.starts_with("HTTPEnable")
            || line.starts_with("HTTPSEnable")
            || line.starts_with("SOCKSEnable"))
            && line.ends_with(": 1")
        {
            return Ok(true);
        }
    }

    Ok(false)
}

/// 清除 macOS 系统代理（HTTP、HTTPS、SOCKS）
/// 遍历所有网络服务，逐一关闭代理
#[tauri::command]
pub async fn clear_macos_system_proxy() -> Result<(), String> {
    let services_out = tokio::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .await
        .map_err(|e| format!("获取网络服务列表失败: {}", e))?;

    let services_text = String::from_utf8_lossy(&services_out.stdout);
    // 第一行是提示语，跳过；带 * 前缀的是已停用的网卡，跳过
    let services: Vec<&str> = services_text
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('*'))
        .collect();

    for svc in &services {
        // 关闭 HTTP 代理
        let _ = tokio::process::Command::new("networksetup")
            .args(["-setwebproxystate", svc, "off"])
            .output()
            .await;
        // 关闭 HTTPS 代理
        let _ = tokio::process::Command::new("networksetup")
            .args(["-setsecurewebproxystate", svc, "off"])
            .output()
            .await;
        // 关闭 SOCKS 代理
        let _ = tokio::process::Command::new("networksetup")
            .args(["-setsocksfirewallproxystate", svc, "off"])
            .output()
            .await;
    }

    Ok(())
}