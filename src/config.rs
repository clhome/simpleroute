use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RouteStatus {
    Inactive,
    Verifying,
    Active,
    Failed(String),
}

impl Default for RouteStatus {
    fn default() -> Self {
        RouteStatus::Inactive
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteItem {
    pub id: String,
    pub name: String,
    pub target: String,
    pub mask: String,
    pub gateway: String,
    pub enabled: bool,
    #[serde(skip)]
    pub status: RouteStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub routes: Vec<RouteItem>,
    pub alarm_limit_mb: f64, // 流量报警限值，单位为 MB/s
    pub enable_alarm: bool,  // 是否开启流量报警
    pub selected_interface_name: Option<String>, // 用户选中的网卡名称，None表示默认选择
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            alarm_limit_mb: 10.0, // 默认 10MB/s
            enable_alarm: false,  // 默认关闭报警
            selected_interface_name: None,
        }
    }
}

pub fn get_config_path() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    let mut path = PathBuf::from(appdata);
    path.push("simpleroute");
    // 确保应用数据目录存在
    let _ = fs::create_dir_all(&path);
    path.push("config.json");
    path
}

pub fn load_config() -> AppConfig {
    let path = get_config_path();
    if !path.exists() {
        return AppConfig::default();
    }
    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(config) => config,
            Err(_) => AppConfig::default(),
        },
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    match serde_json::to_string_pretty(config) {
        Ok(json_str) => match fs::write(&path, json_str) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("写入配置文件失败: {}", e)),
        },
        Err(e) => Err(format!("序列化配置失败: {}", e)),
    }
}
