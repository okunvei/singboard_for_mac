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

    match get_system_proxy().and_then(|url| reqwest::Proxy::all(&url).ok()) {
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
