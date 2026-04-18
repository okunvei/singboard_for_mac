use reqwest;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use lazy_static::lazy_static;
use sysproxy::Sysproxy; // 引入系统代理获取工具

lazy_static! {
    static ref SELF_PROXY: Mutex<String> = Mutex::new(String::new());
}

// 辅助函数：根据优先级获取当前应使用的代理
fn get_effective_proxy() -> Option<reqwest::Proxy> {
    // 1. 最高优先级：检查软件自身设置的代理
    let self_p = SELF_PROXY.lock().unwrap().clone();
    if !self_p.is_empty() {
        return reqwest::Proxy::all(&self_p).ok();
    }

    // 2. 中等优先级：检查 Windows 系统代理
    if let Ok(sys) = Sysproxy::get_system_proxy() {
        if sys.enable && !sys.host.is_empty() {
            let proxy_url = format!("http://{}:{}", sys.host, sys.port);
            return reqwest::Proxy::all(proxy_url).ok();
        }
    }

    // 3. 最低优先级：直连
    None
}

#[tauri::command]
pub fn set_self_proxy(proxy: String) {
    let mut p = SELF_PROXY.lock().unwrap();
    *p = proxy;
    println!("网络模块已收到代理更新: {}", *p);
}

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

    // 应用优先级逻辑
    if let Some(proxy) = get_effective_proxy() {
        builder = builder.proxy(proxy);
    }

    let client = builder.build().map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    resp.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn http_ping(url: String, count: u32) -> Result<f64, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(5));

    // 应用优先级逻辑
    if let Some(proxy) = get_effective_proxy() {
        builder = builder.proxy(proxy);
    }

    let client = builder.build().map_err(|e| e.to_string())?;
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