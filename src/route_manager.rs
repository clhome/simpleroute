use std::net::Ipv4Addr;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use crate::config::{RouteItem, RouteStatus};

// 隐藏控制台窗口的标志
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 验证 IP 地址、子网掩码、网关格式是否合法
pub fn validate_route_params(target: &str, mask: &str, gateway: &str) -> Result<(), String> {
    if Ipv4Addr::from_str(target.trim()).is_err() {
        return Err("目标 IP 地址格式不正确".to_string());
    }
    if Ipv4Addr::from_str(mask.trim()).is_err() {
        return Err("子网掩码格式不正确".to_string());
    }
    if Ipv4Addr::from_str(gateway.trim()).is_err() {
        return Err("网关 IP 地址格式不正确".to_string());
    }
    Ok(())
}

/// 执行带有隐藏窗口的系统命令
fn run_hidden_command(args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("route");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.args(args);

    match cmd.output() {
        Ok(output) => {
            // 虽然 Windows CMD 默认为 GBK 编码，但是我们只需捕获它的返回码，
            // 或者是解析纯 ASCII 字符（如 IP），所以 from_utf8_lossy 足够使用。
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

            if output.status.success() {
                Ok(stdout)
            } else {
                let err_msg = if !stderr.trim().is_empty() {
                    stderr
                } else if !stdout.trim().is_empty() {
                    stdout
                } else {
                    "未知命令执行错误".to_string()
                };
                Err(err_msg)
            }
        }
        Err(e) => Err(format!("无法执行 route 命令: {}", e)),
    }
}

/// 添加静态路由：route add <target> mask <mask> <gateway>
pub fn add_route(route: &RouteItem) -> Result<(), String> {
    let target = route.target.trim();
    let mask = route.mask.trim();
    let gateway = route.gateway.trim();

    validate_route_params(target, mask, gateway)?;

    // 运行 route add 命令
    match run_hidden_command(&["add", target, "mask", mask, gateway]) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("添加路由失败: {}", e)),
    }
}

/// 删除静态路由：route delete <target>
pub fn delete_route(target_ip: &str) -> Result<(), String> {
    let target = target_ip.trim();
    if Ipv4Addr::from_str(target).is_err() {
        return Err("无效的目标 IP 地址，删除终止".to_string());
    }

    match run_hidden_command(&["delete", target]) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("删除路由失败: {}", e)),
    }
}

/// 同步检查路由表是否已存在对应路由（解析 route print -4 输出）
pub fn check_route_active(target: &str, mask: &str, gateway: &str) -> bool {
    let target_ip = target.trim();
    let mask_ip = mask.trim();
    let gw_ip = gateway.trim();

    // 运行 route print -4 并获取输出
    let mut cmd = Command::new("route");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.args(&["print", "-4"]);

    let output = match cmd.output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Err(_) => return false,
    };

    // 逐行解析，确认是否存在匹配的网络目标、掩码和网关
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // 典型匹配行结构：网络目标 | 网络掩码 | 网关 | 接口 | 跃点数
        // 每一行至少应有 5 个部分，并且按顺序包含 target, mask 和 gateway
        if parts.len() >= 4 {
            let row_target = parts[0];
            let row_mask = parts[1];
            let row_gateway = parts[2];

            if row_target == target_ip && row_mask == mask_ip && row_gateway == gw_ip {
                return true;
            }
        }
    }
    false
}

/// 异步验证路由是否生效：延时 2 秒后检查状态并更新
pub fn verify_route_async<F>(route: RouteItem, on_complete: F)
where
    F: FnOnce(RouteStatus) + Send + 'static,
{
    thread::spawn(move || {
        // 延时 2 秒
        thread::sleep(Duration::from_secs(2));

        if check_route_active(&route.target, &route.mask, &route.gateway) {
            on_complete(RouteStatus::Active);
        } else {
            on_complete(RouteStatus::Failed("验证失败: 活跃路由表中未找到匹配条目".to_string()));
        }
    });
}

/// 获取当前系统路由表中的所有手工添加静态路由，并过滤回环、广播、组播和直连网卡路由
pub fn scan_existing_routes() -> Vec<(String, String, String)> {
    let mut cmd = Command::new("route");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.args(&["print", "-4"]);

    let output = match cmd.output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        Err(_) => return Vec::new(),
    };

    let mut routes = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // 匹配包含至少 4 列 IP 信息的行（支持活动路由和永久路由部分的解析）
        if parts.len() >= 4 {
            let target = parts[0];
            let mask = parts[1];
            let gateway = parts[2];

            // 转换并验证 IP
            if let (Ok(t_ip), Ok(_m_ip), Ok(g_ip)) = (
                Ipv4Addr::from_str(target),
                Ipv4Addr::from_str(mask),
                Ipv4Addr::from_str(gateway),
            ) {
                // 1. 过滤默认路由
                if t_ip.is_unspecified() {
                    continue;
                }
                // 2. 过滤本地回环
                if t_ip.is_loopback() {
                    continue;
                }
                // 3. 过滤组播和广播
                if t_ip.is_multicast() || target == "255.255.255.255" {
                    continue;
                }
                // 4. 过滤本地 localhost 网关路由
                if g_ip == Ipv4Addr::new(127, 0, 0, 1) {
                    continue;
                }
                
                // 5. 过滤本地直连路由（在活动路由中，当网关是物理网卡接口 IP 时为直连路由）
                if parts.len() >= 5 {
                    let interface = parts[3];
                    if let Ok(i_ip) = Ipv4Addr::from_str(interface) {
                        if g_ip == i_ip {
                            continue;
                        }
                    }
                }

                let t_str = target.to_string();
                let m_str = mask.to_string();
                let g_str = gateway.to_string();

                // 去重
                if !routes.iter().any(|(t, m, g)| t == &t_str && m == &m_str && g == &g_str) {
                    routes.push((t_str, m_str, g_str));
                }
            }
        }
    }

    routes
}
