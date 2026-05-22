mod config;
mod gui;
mod locale;
mod route_manager;
mod traffic;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use parking_lot::RwLock;

use eframe::egui;
use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent, menu::MenuEvent};

use crate::config::{load_config, save_config, RouteStatus};
use crate::gui::{SharedState, SimplerouteApp};
use crate::traffic::SpeedCalculator;
use crate::route_manager::{check_route_active, add_route};

/// 动态在内存中生成一个 16x16 的渐变彩色圆角 RGBA 像素矩阵作为系统托盘图标，
/// 完全摆脱外部图片文件的依赖，保障打包和运行的绝对稳定。
fn create_default_icon() -> Icon {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let idx = (y * 16 + x) * 4;
            
            // 渐变蓝色
            let r = (x * 10) as u8;
            let g = (y * 10) as u8;
            let b = 220u8;
            let a = 255u8;

            let cx = 8.0;
            let cy = 8.0;
            let dist = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            
            if dist < 7.5 {
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = a;
            } else {
                rgba[idx] = 0;
                rgba[idx + 1] = 0;
                rgba[idx + 2] = 0;
                rgba[idx + 3] = 0; // 圆角之外区域透明
            }
        }
    }
    Icon::from_rgba(rgba, 16, 16).unwrap()
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

    // 初始化系统托盘图标
    let tray_icon = create_default_icon();
    let tray_menu = Menu::new();
    
    let show_item = MenuItem::with_id("show", lang.t("显示主界面", "Open UI Panel"), true, None);
    let recheck_item = MenuItem::with_id("recheck", lang.t("重新检测网络", "Redetect Network"), true, None);
    let exit_item = MenuItem::with_id("exit", lang.t("退出", "Exit"), true, None);
    
    let _ = tray_menu.append(&show_item);
    let _ = tray_menu.append(&recheck_item);
    let _ = tray_menu.append(&exit_item);

    let mut _tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(lang.t("Simpleroute 静态路由托盘管理器", "Simpleroute Tray Manager"))
        .with_icon(tray_icon)
        .build()
        .unwrap();

    // 运行 eframe / egui 视口
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(lang.t("Simpleroute 静态路由托盘管理器", "Simpleroute Route Manager"))
            .with_inner_size([460.0, 520.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_visible(true), // 启动时默认显示主界面，便于直观管理
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
            
            // ==================== 1. 后台流量监控与路由健康自动修复线程 ====================
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
                
                loop {
                    let (alarm_enabled, alarm_limit, selected_name) = {
                        let state = state_for_monitor.read();
                        (state.config.enable_alarm, state.config.alarm_limit_mb, state.config.selected_interface_name.clone())
                    };

                    // 刷新计算网卡总流量速度，传入选中的网卡名称
                    let (rx_speed, tx_speed) = calculator.update(selected_name);
                    
                    {
                        let mut state = state_for_monitor.write();
                        state.current_rx_speed = rx_speed;
                        state.current_tx_speed = tx_speed;
                    }
                    
                    // 流量限值超额弹窗报警
                    if alarm_enabled && alarm_limit > 0.0 && rx_speed > alarm_limit {
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
                                    "当前网卡下载速率 {:.2} MB/s 已超出您的限值设定 ({:.2} MB/s)！",
                                    rx_speed, alarm_limit
                                )
                            } else {
                                format!(
                                    "Current download rate {:.2} MB/s has exceeded your limit setting ({:.2} MB/s)!",
                                    rx_speed, alarm_limit
                                )
                            };
                            let _ = notify_rust::Notification::new()
                                .summary(summary)
                                .body(&body)
                                .timeout(5000)
                                .show();
                        }
                    }

                    // 路由运行状态自检与网络断开自动拉起恢复
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
                            let _ = save_config(&state.config);
                        }
                    }

                    is_first_check = false;

                    // 后台更新后主动唤醒 GUI 刷新界面数据，保持毫秒级流畅刷新
                    ctx_for_monitor.request_repaint();
                    
                    thread::sleep(Duration::from_secs(1));
                }
            });

            // ==================== 2. 系统托盘与菜单项事件监听响应线程 ====================
            let state_for_tray = state_app.clone();
            let ctx_for_tray = egui_ctx.clone();
            
            thread::spawn(move || {
                loop {
                    // 处理托盘事件
                    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                        match event {
                            TrayIconEvent::DoubleClick { .. } => {
                                let state = state_for_tray.read();
                                state.show_window_requested.store(true, Ordering::Relaxed);
                                ctx_for_tray.request_repaint();
                            }
                            _ => {}
                        }
                    }
                    
                    // 处理托盘右键菜单事件
                    while let Ok(event) = MenuEvent::receiver().try_recv() {
                        let state = state_for_tray.read();
                        if event.id == "show" {
                            state.show_window_requested.store(true, Ordering::Relaxed);
                            ctx_for_tray.request_repaint();
                        } else if event.id == "exit" {
                            state.exit_requested.store(true, Ordering::Relaxed);
                            ctx_for_tray.request_repaint();
                        } else if event.id == "recheck" {
                            drop(state); // 先释放读锁，防死锁
                            let mut state = state_for_tray.write();
                            for r in state.config.routes.iter_mut() {
                                if r.enabled {
                                    r.status = RouteStatus::Verifying;
                                }
                            }
                            ctx_for_tray.request_repaint();
                        }
                    }
                    
                    thread::sleep(Duration::from_millis(100));
                }
            });

            Ok(Box::new(SimplerouteApp::new(state_app)))
        }),
    )
}
