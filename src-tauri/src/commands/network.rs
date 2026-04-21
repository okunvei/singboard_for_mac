use reqwest;
use std::time::{Duration, Instant};
use winreg::enums::*;
use winreg::RegKey;

fn get_system_proxy() -> Option<String> {
    let settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;

    let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
    if enabled == 0 {
        return None;
    }

    let server: String = settings.get_value("ProxyServer").ok()?;
    if server.is_empty() {
        return None;
    }

    let addr = if server.contains('=') {
        server
            .split(';')
            .find(|s| s.starts_with("http="))
            .map(|s| s.trim_start_matches("http=").to_string())
            .unwrap_or_else(|| server.split(';').next().unwrap_or("").to_string())
    } else {
        server
    };

    if addr.is_empty() {
        return None;
    }

    if addr.starts_with("http://") || addr.starts_with("https://") {
        Some(addr)
    } else {
        Some(format!("http://{}", addr))
    }
}

fn build_client(timeout: Option<Duration>) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder();

    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }

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
