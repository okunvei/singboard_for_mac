use reqwest;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use lazy_static::lazy_static;

// 1. 创建一个全局的"代理盒子"，让所有网络函数都能读取
lazy_static! {
    static ref SELF_PROXY: Mutex<String> = Mutex::new(String::new());
}

// 2. 接收前端发来的代理设置并存入盒子
#[tauri::command]
pub fn set_self_proxy(proxy: String) {
    let mut p = SELF_PROXY.lock().unwrap();
    *p = proxy;
    println!("网络模块已收到代理更新: {}", *p);
}

/// 从 macOS scutil 读取当前系统代理设置
/// 返回形如 "http://127.0.0.1:7890" 的字符串，没有代理则返回空字符串
#[cfg(target_os = "macos")]
fn get_macos_system_proxy() -> String {
    use std::process::Command;
    let out = match Command::new("scutil").args(["--proxy"]).output() {
        Ok(o) => o,
        Err(_) => return String::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);

    // 解析 HTTPEnable / HTTPProxy / HTTPPort
    let mut http_enabled = false;
    let mut http_host = String::new();
    let mut http_port = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("HTTPEnable") {
            http_enabled = line.ends_with(": 1");
        } else if line.starts_with("HTTPProxy") {
            http_host = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
        } else if line.starts_with("HTTPPort") {
            http_port = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
        }
    }

    if http_enabled && !http_host.is_empty() && !http_port.is_empty() {
        format!("http://{}:{}", http_host, http_port)
    } else {
        String::new()
    }
}

#[cfg(not(target_os = "macos"))]
fn get_macos_system_proxy() -> String {
    String::new()
}

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    let proxy_str = SELF_PROXY.lock().unwrap().clone();

    // 自身代理优先，为空时回退到 macOS 系统代理
    let effective_proxy = if !proxy_str.is_empty() {
        proxy_str
    } else {
        get_macos_system_proxy()
    };

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

    if !effective_proxy.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&effective_proxy) {
            builder = builder.proxy(proxy);
        }
    }

    let client = builder.build().map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    resp.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn http_ping(url: String, count: u32) -> Result<f64, String> {
    let proxy_str = SELF_PROXY.lock().unwrap().clone();

    // 自身代理优先，为空时回退到 macOS 系统代理
    let effective_proxy = if !proxy_str.is_empty() {
        proxy_str
    } else {
        get_macos_system_proxy()
    };

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(5));

    if !effective_proxy.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&effective_proxy) {
            builder = builder.proxy(proxy);
        }
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

/// 检测 macOS 系统代理是否开启（HTTP、HTTPS、SOCKS 任意一个开启即返回 true）
/// 不再依赖配置文件，直接查询系统当前代理状态
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
