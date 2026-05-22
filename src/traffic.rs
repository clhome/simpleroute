use std::ptr;
use std::time::Instant;
use windows_sys::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2};

// Windows 系统回环网卡类型常量值为 24
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

#[derive(Debug, Clone, Copy)]
pub struct TrafficStats {
    pub total_rx: u64, // 累积接收字节
    pub total_tx: u64, // 累积发送字节
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceInfo {
    pub friendly_name: String, // 友好描述名称，例如 "Intel(R) Wi-Fi 6E AX210"
    pub is_wireless: bool,    // 是否是无线网卡
    pub is_active: bool,      // 是否处于连接活跃状态 (OperStatus == 1)
}

/// 列出当前系统除回环网卡之外的所有可用网卡
pub fn list_network_interfaces() -> Vec<InterfaceInfo> {
    let mut p_table: *mut MIB_IF_TABLE2 = ptr::null_mut();
    let ret = unsafe { GetIfTable2(&mut p_table) };
    if ret != 0 {
        return Vec::new();
    }

    let mut list = Vec::new();

    unsafe {
        let num_entries = (*p_table).NumEntries;
        let first_row_ptr = &(*p_table).Table[0] as *const MIB_IF_ROW2;

        for i in 0..num_entries {
            let row_ptr = first_row_ptr.add(i as usize);
            let row = &*row_ptr;

            // 过滤回环网卡 (Type == 24)
            if row.Type != IF_TYPE_SOFTWARE_LOOPBACK {
                let desc_len = row.Description.iter().position(|&c| c == 0).unwrap_or(row.Description.len());
                let friendly_name = String::from_utf16_lossy(&row.Description[..desc_len]);

                list.push(InterfaceInfo {
                    friendly_name,
                    is_wireless: row.Type == 71, // IF_TYPE_IEEE80211 代表无线网卡
                    is_active: row.OperStatus == 1, // 1 表示 OperStatusUp
                });
            }
        }

        FreeMibTable(p_table as *mut std::ffi::c_void);
    }

    list
}

/// 获取指定网卡（或根据默认规则选取）的流量数据
/// 默认选择规则：
/// 1. 优先选择第一个活跃的无线网卡 (Type == 71)；
/// 2. 其次选择第一个活跃的其他物理网卡；
/// 3. 如果依然未找到，则累加所有活跃网卡的总和作为保底。
pub fn get_interface_traffic(selected_name: Option<&str>) -> Result<TrafficStats, String> {
    let mut p_table: *mut MIB_IF_TABLE2 = ptr::null_mut();
    let ret = unsafe { GetIfTable2(&mut p_table) };
    if ret != 0 {
        return Err(format!("GetIfTable2 失败，Windows 错误码: {}", ret));
    }

    let mut total_rx = 0u64;
    let mut total_tx = 0u64;
    let mut found = false;

    unsafe {
        let num_entries = (*p_table).NumEntries;
        let first_row_ptr = &(*p_table).Table[0] as *const MIB_IF_ROW2;

        // 1. 如果用户显式选择了某网卡，尝试寻找匹配友好描述的网卡
        if let Some(target_name) = selected_name {
            for i in 0..num_entries {
                let row_ptr = first_row_ptr.add(i as usize);
                let row = &*row_ptr;

                let desc_len = row.Description.iter().position(|&c| c == 0).unwrap_or(row.Description.len());
                let friendly_name = String::from_utf16_lossy(&row.Description[..desc_len]);

                if friendly_name == target_name {
                    total_rx = row.InOctets;
                    total_tx = row.OutOctets;
                    found = true;
                    break;
                }
            }
        }

        // 2. 如果未指定或指定的网卡不存在，默认显示“第一个活跃的无线网卡” (Type == 71)
        if !found {
            for i in 0..num_entries {
                let row_ptr = first_row_ptr.add(i as usize);
                let row = &*row_ptr;

                if row.Type == 71 && row.OperStatus == 1 {
                    total_rx = row.InOctets;
                    total_tx = row.OutOctets;
                    found = true;
                    break;
                }
            }
        }

        // 3. 如果依然没有活跃无线网卡，退而求其次选择“第一个活跃的物理网卡”
        if !found {
            for i in 0..num_entries {
                let row_ptr = first_row_ptr.add(i as usize);
                let row = &*row_ptr;

                if row.Type != IF_TYPE_SOFTWARE_LOOPBACK && row.OperStatus == 1 {
                    total_rx = row.InOctets;
                    total_tx = row.OutOctets;
                    found = true;
                    break;
                }
            }
        }

        // 4. 极致兜底：如果连活跃物理网卡都没检测到，累加所有非回环网卡的流量，防止返回错误导致崩溃
        if !found {
            for i in 0..num_entries {
                let row_ptr = first_row_ptr.add(i as usize);
                let row = &*row_ptr;

                if row.Type != IF_TYPE_SOFTWARE_LOOPBACK {
                    total_rx += row.InOctets;
                    total_tx += row.OutOctets;
                    found = true;
                }
            }
        }

        FreeMibTable(p_table as *mut std::ffi::c_void);
    }

    if found {
        Ok(TrafficStats { total_rx, total_tx })
    } else {
        Err("系统上未找到任何可用的网卡接口".to_string())
    }
}

/// 支持网卡自选的秒级网速计算器
pub struct SpeedCalculator {
    last_rx: u64,
    last_tx: u64,
    last_time: Instant,
    selected_name: Option<String>,
}

impl SpeedCalculator {
    pub fn new(selected_name: Option<String>) -> Self {
        if let Ok(stats) = get_interface_traffic(selected_name.as_deref()) {
            Self {
                last_rx: stats.total_rx,
                last_tx: stats.total_tx,
                last_time: Instant::now(),
                selected_name,
            }
        } else {
            Self {
                last_rx: 0,
                last_tx: 0,
                last_time: Instant::now(),
                selected_name,
            }
        }
    }

    /// 更新当前流量累积值并计算瞬时速率，返回：(下载速率 MB/s, 上传速率 MB/s)
    /// 传参中需要传入当前最新的用户选中网卡名称
    pub fn update(&mut self, current_selected: Option<String>) -> (f64, f64) {
        let now = Instant::now();

        // 核心保护机制：如果中途切换了网卡，重置统计基准，防止因字节差分爆发出现网速飙升的 Bug
        if current_selected != self.selected_name {
            self.selected_name = current_selected.clone();
            if let Ok(stats) = get_interface_traffic(current_selected.as_deref()) {
                self.last_rx = stats.total_rx;
                self.last_tx = stats.total_tx;
                self.last_time = now;
            }
            return (0.0, 0.0);
        }

        let current_stats = match get_interface_traffic(self.selected_name.as_deref()) {
            Ok(s) => s,
            Err(_) => return (0.0, 0.0),
        };

        let elapsed = now.duration_since(self.last_time).as_secs_f64();
        if elapsed < 0.1 {
            return (0.0, 0.0);
        }

        // 计算当前周期与上个周期累积量的绝对差值
        let rx_diff = current_stats.total_rx.saturating_sub(self.last_rx);
        let tx_diff = current_stats.total_tx.saturating_sub(self.last_tx);

        // 字节数转换为 MB，并除以间隔时间获取速率 MB/s
        let rx_speed_mb = (rx_diff as f64) / (1024.0 * 1024.0) / elapsed;
        let tx_speed_mb = (tx_diff as f64) / (1024.0 * 1024.0) / elapsed;

        // 保存本次状态作为下次计算基准
        self.last_rx = current_stats.total_rx;
        self.last_tx = current_stats.total_tx;
        self.last_time = now;

        (rx_speed_mb, tx_speed_mb)
    }
}
