#![windows_subsystem = "windows"]

mod config;
mod gui;
mod locale;
mod route_manager;
mod traffic;

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use parking_lot::RwLock;

use eframe::egui;
use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};

use crate::config::{load_config, save_config, RouteStatus};
use crate::gui::{SharedState, SimplerouteApp};
use crate::traffic::SpeedCalculator;
use crate::route_manager::{check_route_active, add_route};

/// 动态在内存中生成一个 16x16 的渐变彩色圆角 RGBA 像素矩阵作为系统托盘图标，
/// 完全摆脱外部图片文件的依赖，保障打包和运行的绝对稳定。
fn create_default_icon() -> Icon {
    let png_bytes = include_bytes!("img/logo32.png");
    let decoder = png::Decoder::new(&png_bytes[..]);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let rgba = buf[..info.buffer_size()].to_vec();
    Icon::from_rgba(rgba, info.width, info.height).unwrap()
}

fn load_window_icon() -> egui::IconData {
    let png_bytes = include_bytes!("img/logo64.png");
    let decoder = png::Decoder::new(&png_bytes[..]);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let rgba = buf[..info.buffer_size()].to_vec();
    egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    }
}

/// 全局存储主窗口的 Win32 HWND 句柄，供后台线程直接通过 OS 原生 API 操控窗口显隐
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

/// 通过 Win32 FindWindowW 搜索主窗口句柄并缓存到全局静态变量
#[cfg(target_os = "windows")]
fn find_and_cache_hwnd(title: &str) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = std::ffi::OsStr::new(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
            std::ptr::null(),
            wide.as_ptr(),
        );
        if hwnd != 0 {
            MAIN_HWND.store(hwnd, Ordering::Relaxed);
        }
    }
}

/// 通过 Win32 原生 API 直接强行拉起并前置主窗口（完全绕过 eframe/egui 内部管道）
#[cfg(target_os = "windows")]
fn win32_show_window() {
    let hwnd = MAIN_HWND.load(Ordering::Relaxed);
    if hwnd != 0 {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SetForegroundWindow, SW_SHOW};
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }
}

fn main() -> eframe::Result {
    // 自动检测系统 UI 语言
    let lang = crate::locale::Language::detect();

    // 载入持久化配置
    let mut config = load_config();
    
    // 程序启动时，首先扫描并同步当前电脑上已经手工配置好的静态路由
    let existing_routes = crate::route_manager::scan_existing_routes();
    let mut config_changed = false;
    for (target, mask, gateway) in existing_routes {
        // 如果在当前配置的路由中，没有找到相同目标、掩码和网关的项：
        if !config.routes.iter().any(|r| r.target == target && r.mask == mask && r.gateway == gateway) {
            let name = if lang == crate::locale::Language::Zh {
                format!("已同步-{}", target)
            } else {
                format!("Synced-{}", target)
            };
            // 毫秒级时间戳确保 ID 唯一性
            let id = format!("{}-{}", target.replace(".", "-"), chrono::Local::now().timestamp_millis());
            let new_item = crate::config::RouteItem {
                id,
                name,
                target,
                mask,
                gateway,
                enabled: true, // 已存在的静态路由，默认设为启用状态
                status: RouteStatus::Active, // 已经在路由表，直接标记为生效
            };
            config.routes.push(new_item);
            config_changed = true;
        }
    }
    if config_changed {
        let _ = save_config(&config);
    }
    
    // 程序启动时，自动尝试重新添加所有之前开启的静态路由，实现“断线/重启自动恢复”
    for route in config.routes.iter_mut() {
        if route.enabled {
            route.status = RouteStatus::Verifying;
            let r_clone = route.clone();
            // 在独立线程运行，防止阻塞主初始化流程
            thread::spawn(move || {
                let _ = add_route(&r_clone);
            });
        } else {
            route.status = RouteStatus::Inactive;
        }
    }

    // 初始化全局共享状态
    let show_window_requested = Arc::new(AtomicBool::new(false));
    let exit_requested = Arc::new(AtomicBool::new(false));

    let shared_state = Arc::new(RwLock::new(SharedState {
        config,
        current_rx_speed: 0.0,
        current_tx_speed: 0.0,
        show_window_requested: show_window_requested.clone(),
        exit_requested: exit_requested.clone(),
        lang,
    }));



    // 运行 eframe / egui 视口
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(lang.t("Simpleroute 静态路由托盘管理器", "Simpleroute Route Manager"))
            .with_inner_size([460.0, 570.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_decorations(false) // 隐藏 Windows 默认窗口头部边框
            .with_icon(load_window_icon())
            .with_visible(false), // 启动时默认静默隐藏到托盘，不打扰用户
        ..Default::default()
    };

    let state_app = shared_state.clone();
    
    eframe::run_native(
        "Simpleroute",
        options,
        Box::new(move |cc| {
            let egui_ctx = cc.egui_ctx.clone();
            
            // 解决 egui 默认英文字体渲染中文产生的方框乱码，载入系统自带中文字体
            crate::gui::setup_chinese_font(&egui_ctx);
            
            // ==================== 1. 主线程托管系统托盘与菜单 ====================
            let lang = {
                let state = state_app.read();
                state.lang
            };

            let tray_icon = create_default_icon();
            let tray_menu = Menu::new();
            
            let show_item = MenuItem::with_id("show", lang.t("显示主界面", "Open UI Panel"), true, None);
            let recheck_item = MenuItem::with_id("recheck", lang.t("重新检测网络", "Redetect Network"), true, None);
            let exit_item = MenuItem::with_id("exit", lang.t("退出", "Exit"), true, None);
            
            let _ = tray_menu.append(&show_item);
            let _ = tray_menu.append(&recheck_item);
            let _ = tray_menu.append(&exit_item);

            // 在主线程内创建系统托盘，它的隐藏窗口将自动归属于主线程的消息循环
            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_tooltip(lang.t("Simpleroute 静态路由托盘管理器", "Simpleroute Tray Manager"))
                .with_icon(tray_icon)
                .build()
                .unwrap();

            // 缓存主窗口的 Win32 HWND 句柄（此时窗口已由 eframe 创建完毕）
            #[cfg(target_os = "windows")]
            {
                let window_title = lang.t("Simpleroute 静态路由托盘管理器", "Simpleroute Route Manager");
                find_and_cache_hwnd(window_title);
            }

            // 启动专属的后台托盘菜单事件接收线程（阻塞式读取，对 CPU 极其友好）
            let ctx_clone = egui_ctx.clone();
            let state_clone = state_app.clone();
            thread::spawn(move || {
                while let Ok(event) = tray_icon::menu::MenuEvent::receiver().recv() {
                    if event.id == "show" {
                        // 防御性地重新查找并缓存一次主窗口 HWND，确保启动隐藏状态下也能 100% 成功拉起
                        #[cfg(target_os = "windows")]
                        {
                            let state = state_clone.read();
                            let window_title = state.lang.t("Simpleroute 静态路由托盘管理器", "Simpleroute Route Manager");
                            find_and_cache_hwnd(window_title);
                        }

                        // 通过 Win32 原生 API 直接拉起窗口（完全绕过 eframe 隐藏时的内部停滞）
                        #[cfg(target_os = "windows")]
                        win32_show_window();
                        // 同步 eframe 内部状态并请求重绘
                        ctx_clone.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx_clone.send_viewport_cmd(egui::ViewportCommand::Focus);
                        ctx_clone.request_repaint();
                    } else if event.id == "exit" {
                        // 先保存配置防止数据丢失，然后强行退出进程
                        {
                            let state = state_clone.read();
                            let _ = save_config(&state.config);
                        }
                        std::process::exit(0);
                    } else if event.id == "recheck" {
                        {
                            let mut state = state_clone.write();
                            for r in state.config.routes.iter_mut() {
                                if r.enabled {
                                    r.status = RouteStatus::Verifying;
                                }
                            }
                        }
                        ctx_clone.request_repaint();
                    }
                }
            });

            // 启动专属的后台托盘双击事件接收线程
            let ctx_clone2 = egui_ctx.clone();
            let state_clone2 = state_app.clone();
            thread::spawn(move || {
                while let Ok(event) = tray_icon::TrayIconEvent::receiver().recv() {
                    match event {
                        tray_icon::TrayIconEvent::DoubleClick { .. } => {
                            // 防御性地重新查找并缓存一次主窗口 HWND，确保启动隐藏状态下也能 100% 成功拉起
                            #[cfg(target_os = "windows")]
                            {
                                let state = state_clone2.read();
                                let window_title = state.lang.t("Simpleroute 静态路由托盘管理器", "Simpleroute Route Manager");
                                find_and_cache_hwnd(window_title);
                            }

                            // 通过 Win32 原生 API 直接拉起窗口
                            #[cfg(target_os = "windows")]
                            win32_show_window();
                            ctx_clone2.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            ctx_clone2.send_viewport_cmd(egui::ViewportCommand::Focus);
                            ctx_clone2.request_repaint();
                        }
                        _ => {}
                    }
                }
            });
            
            // ==================== 2. 后台流量监控与路由健康自动修复线程 ====================
            let state_for_monitor = state_app.clone();
            let ctx_for_monitor = egui_ctx.clone();
            
            thread::spawn(move || {
                let init_selected = {
                    let state = state_for_monitor.read();
                    state.config.selected_interface_name.clone()
                };
                let mut calculator = SpeedCalculator::new(init_selected);
                let mut last_alarm_time: Option<Instant> = None;
                
                // 获取系统的语言
                let lang = {
                    let state = state_for_monitor.read();
                    state.lang
                };
                let mut is_first_check = true;
                let mut save_counter = 0; // 存盘防抖计数器
                
                loop {
                    let (alarm_enabled, alarm_limit, selected_name) = {
                        let state = state_for_monitor.read();
                        (state.config.enable_alarm, state.config.alarm_limit_mb, state.config.selected_interface_name.clone())
                    };

                    // 刷新计算网卡总流量速度，传入选中的网卡名称
                    let (rx_speed, tx_speed) = calculator.update(selected_name);
                    
                    let mut config_save_needed = false;
                    
                    {
                        let mut state = state_for_monitor.write();
                        state.current_rx_speed = rx_speed;
                        state.current_tx_speed = tx_speed;
                        
                        // 1. 流量累加及日期跨天检测
                        let current_date = chrono::Local::now().format("%Y-%m-%d").to_string();
                        if state.config.last_traffic_date.is_empty() {
                            state.config.last_traffic_date = current_date;
                            config_save_needed = true;
                        } else if state.config.last_traffic_date != current_date {
                            state.config.yesterday_traffic_mb = state.config.today_traffic_mb;
                            state.config.today_traffic_mb = 0.0;
                            state.config.last_traffic_date = current_date;
                            config_save_needed = true;
                        }
                        
                        // 当前秒产生流量累加到今日流量中 (MB)
                        let current_second_traffic_mb = rx_speed + tx_speed;
                        state.config.today_traffic_mb += current_second_traffic_mb;
                    }
                    
                    // 2. Mbps 级别流量超限报警校验 (1 MB/s = 8 Mbps)
                    let rx_speed_mbps = rx_speed * 8.0;
                    if alarm_enabled && alarm_limit > 0.0 && rx_speed_mbps > alarm_limit {
                        let now = Instant::now();
                        let should_alarm = match last_alarm_time {
                            None => true,
                            Some(last) => now.duration_since(last) > Duration::from_secs(30), // 30秒报警静默期，防轰炸
                        };

                        if should_alarm {
                            last_alarm_time = Some(now);
                            // 弹出系统右下角气泡消息
                            let summary = lang.t("⚠️ Simpleroute 流量报警", "⚠️ Simpleroute Traffic Alarm");
                            let body = if lang == crate::locale::Language::Zh {
                                format!(
                                    "当前网卡下载速率 {:.2} Mbps 已超出您的限值设定 ({:.2} Mbps)！",
                                    rx_speed_mbps, alarm_limit
                                )
                            } else {
                                format!(
                                    "Current download rate {:.2} Mbps has exceeded your limit setting ({:.2} Mbps)!",
                                    rx_speed_mbps, alarm_limit
                                )
                            };
                            let _ = notify_rust::Notification::new()
                                .summary(summary)
                                .body(&body)
                                .timeout(5000)
                                .show();
                        }
                    }

                    // 3. 路由运行状态自检与网络断开自动拉起恢复
                    {
                        let mut state = state_for_monitor.write();
                        let mut changed = false;
                        
                        for route in state.config.routes.iter_mut() {
                            if route.enabled {
                                match route.status {
                                    RouteStatus::Verifying | RouteStatus::Inactive => {
                                        if check_route_active(&route.target, &route.mask, &route.gateway) {
                                            let prev_status = route.status.clone();
                                            route.status = RouteStatus::Active;
                                            changed = true;

                                            // 非首次检测，且是从验证中恢复为已生效，说明是断网重拉恢复成功！
                                            if !is_first_check && prev_status == RouteStatus::Verifying {
                                                let r_name = route.name.clone();
                                                let summary = lang.t("🔄 Simpleroute 路由已重建", "🔄 Simpleroute Route Rebuilt");
                                                let body = if lang == crate::locale::Language::Zh {
                                                    format!("静态路由 [{}] 意外丢失已在 1 秒内自动静默重建并生效！", r_name)
                                                } else {
                                                    format!("Static route [{}] was lost and has been rebuilt silently within 1 second!", r_name)
                                                };
                                                let _ = notify_rust::Notification::new()
                                                    .summary(summary)
                                                    .body(&body)
                                                    .timeout(4000)
                                                    .show();
                                            }
                                        } else if route.status == RouteStatus::Inactive {
                                            // 处于激活标志却未检测到路由（可能断网后恢复），重新下发路由指令
                                            route.status = RouteStatus::Verifying;
                                            let r_clone = route.clone();
                                            thread::spawn(move || {
                                                let _ = add_route(&r_clone);
                                            });
                                        }
                                    }
                                    RouteStatus::Active => {
                                        // 定期健康检查，若意外丢失，置为 Verifying 并自动重连修复！
                                        if !check_route_active(&route.target, &route.mask, &route.gateway) {
                                            route.status = RouteStatus::Verifying;
                                            changed = true;
                                            let r_clone = route.clone();
                                            thread::spawn(move || {
                                                let _ = add_route(&r_clone);
                                            });
                                        }
                                    }
                                    RouteStatus::Failed(_) => {}
                                }
                            } else {
                                route.status = RouteStatus::Inactive;
                            }
                        }
                        
                        if changed {
                            config_save_needed = true;
                        }
                    }

                    is_first_check = false;

                    // 4. 定期防抖存盘逻辑（合并流量保存和路由状态保存，10秒一次，保障性能）
                    save_counter += 1;
                    if save_counter >= 10 || config_save_needed {
                        save_counter = 0;
                        let state = state_for_monitor.read();
                        let _ = save_config(&state.config);
                    }

                    // 后台更新后主动唤醒 GUI 刷新界面数据，保持毫秒级流畅刷新
                    ctx_for_monitor.request_repaint();
                    
                    thread::sleep(Duration::from_secs(1));
                }
            });

            Ok(Box::new(SimplerouteApp::new(state_app, tray)))
        }),
    )
}
