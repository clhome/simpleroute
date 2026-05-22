use eframe::egui;
use std::net::Ipv4Addr;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use parking_lot::RwLock;

use crate::config::{save_config, RouteItem, RouteStatus, AppConfig};
use crate::route_manager::{add_route, delete_route};
use crate::locale::Language;

// 共享的应用程序状态
pub struct SharedState {
    pub config: AppConfig,
    pub current_rx_speed: f64,
    pub current_tx_speed: f64,
    pub show_window_requested: Arc<std::sync::atomic::AtomicBool>,
    pub exit_requested: Arc<std::sync::atomic::AtomicBool>,
    pub lang: Language,
}

pub struct SimplerouteApp {
    pub state: Arc<RwLock<SharedState>>,
    
    // 临时输入表单变量
    new_name: String,
    new_target: String,
    new_mask: String,
    new_gateway: String,
    
    error_message: Option<String>,
    success_message: Option<String>,
    message_timer: f64,
    
    logo_texture: Option<egui::TextureHandle>, // 缓存公司 Logo 图像纹理
    
    _tray: tray_icon::TrayIcon, // 用来维持托盘的生命周期，加下划线避免 unused 警告
    last_tooltip_text: String, // 缓存上次设置的托盘 Tooltip，避免每一帧重复调用
    first_frame_hidden: bool,  // 是否已在第一帧强制进行隐藏，以实现静默启动
}

impl SimplerouteApp {
    pub fn new(state: Arc<RwLock<SharedState>>, tray: tray_icon::TrayIcon) -> Self {
        Self {
            state,
            new_name: String::new(),
            new_target: String::new(),
            new_mask: "255.255.255.255".to_string(), // 默认为单主机掩码
            new_gateway: String::new(),
            error_message: None,
            success_message: None,
            message_timer: 0.0,
            logo_texture: None,
            _tray: tray,
            last_tooltip_text: String::new(),
            first_frame_hidden: false,
        }
    }

    /// 显示提示信息并在数秒后自动消除
    fn show_info_messages(&mut self, ui: &mut egui::Ui, dt: f64) {
        if self.error_message.is_some() || self.success_message.is_some() {
            self.message_timer -= dt;
            if self.message_timer <= 0.0 {
                self.error_message = None;
                self.success_message = None;
            }
        }

        if let Some(ref err) = self.error_message {
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(80, 20, 20, 200))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 50, 50)))
                .rounding(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("❌").size(14.0));
                        ui.label(egui::RichText::new(err).color(egui::Color32::WHITE).size(13.0));
                    });
                });
            ui.add_space(8.0);
        }

        if let Some(ref msg) = self.success_message {
            egui::Frame::none()
                .fill(egui::Color32::from_rgba_unmultiplied(20, 80, 20, 200))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 180, 50)))
                .rounding(4.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("✅").size(14.0));
                        ui.label(egui::RichText::new(msg).color(egui::Color32::WHITE).size(13.0));
                    });
                });
            ui.add_space(8.0);
        }
    }

    fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
        self.success_message = None;
        self.message_timer = 5.0; // 5秒后自动关闭
    }

    fn set_success(&mut self, msg: String) {
        self.success_message = Some(msg);
        self.error_message = None;
        self.message_timer = 4.0; // 4秒后自动关闭
    }
}

/// 精美圆角 Toggle 切换开关小部件
fn custom_toggle_ui(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let desired_size = egui::vec2(38.0, 18.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool(response.id, *on);
        
        // 交互颜色设计
        let bg_color = if *on {
            egui::Color32::from_rgb(46, 125, 50) // 开启时：典雅森林绿
        } else {
            egui::Color32::from_rgb(70, 70, 80)  // 关闭时：现代深灰
        };

        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, bg_color);

        // 绘制滑块圆形
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter().circle_filled(center, radius - 2.0, egui::Color32::WHITE);
    }
    response
}

impl eframe::App for SimplerouteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 第一次渲染帧时，强制发送 Visible(false) 命令，以确保无论 winit/UAC 初始化状态如何，窗口都能静默隐藏启动
        if !self.first_frame_hidden {
            self.first_frame_hidden = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // 懒加载公司 logo 纹理
        let logo_tex = self.logo_texture.get_or_insert_with(|| {
            let png_bytes = include_bytes!("img/logo32.png");
            let decoder = png::Decoder::new(&png_bytes[..]);
            let mut reader = decoder.read_info().unwrap();
            let mut buf = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut buf).unwrap();
            let rgba = buf[..info.buffer_size()].to_vec();
            
            ctx.load_texture(
                "logo32",
                egui::ColorImage::from_rgba_unmultiplied(
                    [info.width as usize, info.height as usize],
                    &rgba,
                ),
                Default::default()
            )
        }).clone();

        // 拦截点击右上角“关闭”按钮事件：取消窗口真正的退出关闭，改为仅将其隐藏
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // 每 200 毫秒重绘一次，以保证网速与路由状态的顺滑刷新
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        // 检查后台线程是否请求唤醒主窗口
        {
            let state = self.state.read();
            if state.show_window_requested.load(Ordering::Relaxed) {
                state.show_window_requested.store(false, Ordering::Relaxed);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            if state.exit_requested.load(Ordering::Relaxed) {
                // 直接强行退出进程，避免 ViewportCommand::Close 被 close_requested 拦截
                let _ = crate::config::save_config(&state.config);
                std::process::exit(0);
            }
        }

        // 读取当前全局网速、配置与今日/昨日流量
        let (rx_speed, tx_speed, mut alarm_limit, mut alarm_enabled, lang, today_traffic, yesterday_traffic) = {
            let state = self.state.read();
            (
                state.current_rx_speed,
                state.current_tx_speed,
                state.config.alarm_limit_mb,
                state.config.enable_alarm,
                state.lang,
                state.config.today_traffic_mb,
                state.config.yesterday_traffic_mb,
            )
        };

        // 动态更新系统托盘的 Tooltip，仅在内容确实改变时才更新，降低系统调用开销
        let tooltip_text = {
            let today_traffic_str = format_traffic(today_traffic);
            let diff_percent_str = if yesterday_traffic == 0.0 {
                if today_traffic == 0.0 {
                    "0%".to_string()
                } else {
                    "+100%".to_string()
                }
            } else {
                let diff = (today_traffic - yesterday_traffic) / yesterday_traffic * 100.0;
                if diff > 0.0 {
                    format!("+{:.0}%", diff)
                } else if diff < 0.0 {
                    format!("{:.0}%", diff)
                } else {
                    "0%".to_string()
                }
            };

            match lang {
                // 压缩字数以完美适配 Windows 托盘单行显示阈值，避免系统强制换行
                Language::Zh => format!("御风 simpleroute 今日流量：{}（{}）", today_traffic_str, diff_percent_str),
                Language::En => format!("YF simpleroute Today: {} ({})", today_traffic_str, diff_percent_str),
            }
        };

        if tooltip_text != self.last_tooltip_text {
            let _ = self._tray.set_tooltip(Some(&tooltip_text));
            self.last_tooltip_text = tooltip_text;
        }

        // UI 风格设置：采用极富科技感的暗色微光色调
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(18, 18, 22);
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(26, 26, 32);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);
        ctx.set_visuals(visuals);

        egui::CentralPanel::default().show(ctx, |ui| {
            let dt = ui.input(|i| i.stable_dt) as f64;
            
            // 顶部自定义精美无边框标题栏 (嵌入公司专属图标，支持视口平滑拖拽)
            let header_response = egui::Frame::none()
                .fill(egui::Color32::from_rgb(26, 26, 32))
                .inner_margin(egui::Margin {
                    left: 10.0,
                    right: 10.0,
                    top: 8.0,
                    bottom: 8.0,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // 嵌入公司 Logo 纹理
                        ui.image((logo_tex.id(), egui::vec2(16.0, 16.0)));
                        ui.add_space(4.0);
                        // 标题名称
                        ui.label(egui::RichText::new("Simpleroute").strong().color(egui::Color32::from_rgb(0, 180, 216)).size(14.0));
                        ui.label(egui::RichText::new(lang.t("静态路由托盘管理器", "Tray Manager")).size(11.0).color(egui::Color32::from_rgb(130, 130, 140)));
                        
                        // 窗口操作按钮：关闭按钮 (隐藏到托盘)
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // 关闭按钮：字号放大并扩展 Hitbox 交互感应热区至 24x24 像素，显著提升点击命中率
                            let close_btn = ui.add(
                                egui::Button::new(egui::RichText::new("❌").size(12.0))
                                    .frame(false)
                                    .min_size(egui::vec2(24.0, 24.0))
                            );
                            if close_btn.clicked() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                            }
                        });
                    });
                }).response;

            // 剔除右侧 36 像素的关闭按钮区域，在左侧绝大部分标题栏区域内注册专属拖拽交互，提供极佳的手感
            let mut drag_rect = header_response.rect;
            drag_rect.max.x -= 36.0;
            let drag_response = ui.interact(drag_rect, ui.id().with("header_drag"), egui::Sense::drag());
            if drag_response.dragged() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            ui.add_space(4.0);

            // 提示信息栏
            self.show_info_messages(ui, dt);

            // 第一区域：网卡流量仪表盘
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(26, 26, 32))
                .rounding(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(lang.t("⚡ 网卡实时流量统计", "⚡ Real-time Network Traffic")).size(13.0).strong().color(egui::Color32::from_rgb(200, 200, 210)));
                        
                        let current_selected = {
                            self.state.read().config.selected_interface_name.clone()
                        };
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let interfaces = crate::traffic::list_network_interfaces();
                            let selected_text = current_selected.clone().unwrap_or_else(|| {
                                let default_wireless = interfaces.iter().find(|i| i.is_wireless && i.is_active);
                                if let Some(dw) = default_wireless {
                                    if lang == Language::Zh {
                                        format!("📶 自动(无线): {}", dw.friendly_name)
                                    } else {
                                        format!("📶 Auto(Wifi): {}", dw.friendly_name)
                                    }
                                } else if let Some(first_active) = interfaces.iter().find(|i| i.is_active) {
                                    if lang == Language::Zh {
                                        format!("🔌 自动(有线): {}", first_active.friendly_name)
                                    } else {
                                        format!("🔌 Auto(Wired): {}", first_active.friendly_name)
                                    }
                                } else {
                                    if lang == Language::Zh {
                                        "🔍 自动选择".to_string()
                                    } else {
                                        "🔍 Auto Select".to_string()
                                    }
                                }
                            });
                            
                            egui::ComboBox::new("interface_select", "")
                                .selected_text(&truncate_str(&selected_text, 16))
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    let is_default = current_selected.is_none();
                                    if ui.selectable_label(is_default, lang.t("💡 默认无线网卡优先 (自动)", "💡 Default Wireless Card Priority (Auto)")).clicked() {
                                        {
                                            let mut state = self.state.write();
                                            state.config.selected_interface_name = None;
                                            let _ = save_config(&state.config);
                                        }
                                        self.set_success(lang.t("已切换为默认无线网卡优先模式！", "Switched to default wireless card priority mode!").to_string());
                                    }
                                    
                                    ui.separator();
                                    
                                    for interface in interfaces {
                                        let icon = if interface.is_wireless { "📶" } else { "🔌" };
                                        let status_label = if interface.is_active {
                                            lang.t("已连接", "Connected")
                                        } else {
                                            lang.t("未连接", "Disconnected")
                                        };
                                        let label_text = format!("{} {} ({})", icon, interface.friendly_name, status_label);
                                        
                                        let is_selected = current_selected.as_ref() == Some(&interface.friendly_name);
                                        if ui.selectable_label(is_selected, label_text).clicked() {
                                            {
                                                let mut state = self.state.write();
                                                state.config.selected_interface_name = Some(interface.friendly_name.clone());
                                                let _ = save_config(&state.config);
                                            }
                                            let success_msg = if lang == Language::Zh {
                                                format!("已成功切换监听网卡为：{}", interface.friendly_name)
                                            } else {
                                                format!("Successfully switched interface to: {}", interface.friendly_name)
                                            };
                                            self.set_success(success_msg);
                                        }
                                    }
                                });
                        });
                    });
                    ui.add_space(6.0);
                    
                    ui.columns(2, |columns| {
                        // 接收流量卡片
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgb(32, 40, 45))
                            .rounding(6.0)
                            .inner_margin(8.0)
                            .show(&mut columns[0], |ui| {
                                ui.label(egui::RichText::new(lang.t("📥 下载速率", "📥 Download Rate")).size(11.0).color(egui::Color32::from_rgb(120, 180, 200)));
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(format!("{:.2} Mbps", rx_speed * 8.0)).size(20.0).strong().color(egui::Color32::from_rgb(0, 180, 160)));
                            });
                        
                        // 发送流量卡片
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgb(40, 32, 45))
                            .rounding(6.0)
                            .inner_margin(8.0)
                            .show(&mut columns[1], |ui| {
                                ui.label(egui::RichText::new(lang.t("📤 上传速率", "📤 Upload Rate")).size(11.0).color(egui::Color32::from_rgb(180, 120, 200)));
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(format!("{:.2} Mbps", tx_speed * 8.0)).size(20.0).strong().color(egui::Color32::from_rgb(180, 50, 180)));
                            });
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // 流量报警阈值设置
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(lang.t("流量超额弹窗报警:", "Traffic Limit Alert:")).size(12.0));
                        if custom_toggle_ui(ui, &mut alarm_enabled).changed() {
                            let mut state = self.state.write();
                            state.config.enable_alarm = alarm_enabled;
                            let _ = save_config(&state.config);
                        }
                        
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new(lang.t("报警值 (Mbps):", "Alarm Value (Mbps):")).size(12.0));
                        
                        let mut alarm_limit_str = format!("{:.1}", alarm_limit);
                        ui.add(egui::TextEdit::singleline(&mut alarm_limit_str).desired_width(50.0));
                        if let Ok(new_limit) = f64::from_str(&alarm_limit_str) {
                            if new_limit != alarm_limit {
                                alarm_limit = new_limit;
                                let mut state = self.state.write();
                                state.config.alarm_limit_mb = alarm_limit;
                                let _ = save_config(&state.config);
                            }
                        }
                    });
                });

            ui.add_space(10.0);

            // 第二区域：静态路由管理列表
            ui.label(egui::RichText::new(lang.t("📌 静态路由配置列表", "📌 Static Route Configurations")).size(13.0).strong().color(egui::Color32::from_rgb(200, 200, 210)));
            ui.add_space(4.0);

            let mut routes_to_delete = Vec::new();
            let mut route_toggle_events = Vec::new();

            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                let mut state = self.state.write();
                let routes = &mut state.config.routes;

                if routes.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new(lang.t("暂无路由配置，可在下方表单新增", "No configuration found, add one below")).color(egui::Color32::GRAY).italics());
                        ui.add_space(20.0);
                    });
                } else {
                    for (index, route) in routes.iter_mut().enumerate() {
                        let card_color = if route.enabled {
                            egui::Color32::from_rgb(24, 28, 38) // 开启时：科技微蓝背景
                        } else {
                            egui::Color32::from_rgb(22, 22, 26)  // 关闭时：沉静深灰背景
                        };

                        egui::Frame::none()
                            .fill(card_color)
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 55)))
                            .rounding(6.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // 别名与基本信息
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(&route.name).size(13.0).strong().color(egui::Color32::WHITE));
                                            
                                            // 生效状态小药丸
                                            match route.status {
                                                RouteStatus::Inactive => {
                                                    ui.label(egui::RichText::new(lang.t(" 未启用 ", " Inactive ")).size(10.0).background_color(egui::Color32::from_rgb(70, 70, 75)).color(egui::Color32::LIGHT_GRAY));
                                                }
                                                RouteStatus::Verifying => {
                                                    ui.label(egui::RichText::new(lang.t(" ⏳ 验证中 ", " ⏳ Verifying ")).size(10.0).background_color(egui::Color32::from_rgb(13, 71, 161)).color(egui::Color32::WHITE));
                                                }
                                                RouteStatus::Active => {
                                                    ui.label(egui::RichText::new(lang.t(" ✅ 已生效 ", " ✅ Active ")).size(10.0).background_color(egui::Color32::from_rgb(46, 125, 50)).color(egui::Color32::WHITE));
                                                }
                                                RouteStatus::Failed(ref err) => {
                                                    let label = ui.label(egui::RichText::new(lang.t(" ❌ 失败 ", " ❌ Failed ")).size(10.0).background_color(egui::Color32::from_rgb(198, 40, 40)).color(egui::Color32::WHITE));
                                                    
                                                    let err_text = if lang == Language::Zh {
                                                        let err_zh = if err.contains("验证失败: 活跃路由表中未找到匹配条目") {
                                                            "验证失败: 活跃路由表中未找到匹配条目"
                                                        } else {
                                                            err
                                                        };
                                                        format!("详细原因: {}", err_zh)
                                                    } else {
                                                        let err_en = if err.contains("验证失败: 活跃路由表中未找到匹配条目") {
                                                            "Verification failed: No matching entry in routing table"
                                                        } else {
                                                            err
                                                        };
                                                        format!("Reason: {}", err_en)
                                                    };
                                                    label.on_hover_text(err_text);
                                                }
                                            }
                                        });
                                        ui.add_space(2.0);
                                        let detail_text = if lang == Language::Zh {
                                            format!("目标: {}  掩码: {}  网关: {}", route.target, route.mask, route.gateway)
                                        } else {
                                            format!("Target: {}  Mask: {}  Gateway: {}", route.target, route.mask, route.gateway)
                                        };
                                        ui.label(egui::RichText::new(detail_text).size(11.0).color(egui::Color32::from_rgb(170, 170, 180)));
                                    });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        // 删除按钮
                                        if ui.button(egui::RichText::new("🗑").color(egui::Color32::from_rgb(220, 80, 80))).clicked() {
                                            routes_to_delete.push(index);
                                        }

                                        ui.add_space(8.0);

                                        // 开关
                                        let mut is_enabled = route.enabled;
                                        if custom_toggle_ui(ui, &mut is_enabled).changed() {
                                            route_toggle_events.push((index, is_enabled));
                                        }
                                    });
                                });
                            });
                        ui.add_space(4.0);
                    }
                }
            });

            // 处理路由删除与快捷开关事件（在释放锁后运行系统命令，避免锁竞争死锁）
            if !routes_to_delete.is_empty() || !route_toggle_events.is_empty() {
                let mut state = self.state.write();
                
                // 处理删除
                for &index in routes_to_delete.iter().rev() {
                    let route = state.config.routes.remove(index);
                    if route.enabled {
                        let target = route.target.clone();
                        thread::spawn(move || {
                            let _ = delete_route(&target);
                        });
                    }
                }

                // 处理快捷开关
                for (index, new_val) in route_toggle_events {
                    if let Some(route) = state.config.routes.get_mut(index) {
                        route.enabled = new_val;
                        if new_val {
                            route.status = RouteStatus::Verifying;
                            let route_clone = route.clone();
                            let state_clone = self.state.clone();
                            
                            // 异步调用系统命令添加路由，完成后触发异步验证
                            thread::spawn(move || {
                                match add_route(&route_clone) {
                                    Ok(_) => {
                                        // 开启 2 秒后验证
                                        let state_for_verify = state_clone.clone();
                                        let target = route_clone.target.clone();
                                        crate::route_manager::verify_route_async(route_clone, move |status| {
                                            let mut st = state_for_verify.write();
                                            if let Some(r) = st.config.routes.iter_mut().find(|r| r.target == target) {
                                                r.status = status;
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        let mut st = state_clone.write();
                                        if let Some(r) = st.config.routes.iter_mut().find(|r| r.target == route_clone.target) {
                                            r.status = RouteStatus::Failed(e);
                                        }
                                    }
                                }
                            });
                        } else {
                            route.status = RouteStatus::Inactive;
                            let target = route.target.clone();
                            thread::spawn(move || {
                                let _ = delete_route(&target);
                            });
                        }
                    }
                }
                
                let _ = save_config(&state.config);
            }

            ui.add_space(8.0);

            // 第三区域：配置快捷添加表单
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(26, 26, 32))
                .rounding(8.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(lang.t("➕ 添加新静态路由", "➕ Add New Static Route")).size(12.0).strong().color(egui::Color32::from_rgb(0, 180, 216)));
                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(lang.t("目标 IP *", "Target IP *")).size(10.0));
                            ui.add(egui::TextEdit::singleline(&mut self.new_target).hint_text(lang.t("例如 3.3.3.3", "e.g. 3.3.3.3")).desired_width(100.0));
                        });
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(lang.t("子网掩码 *", "Netmask *")).size(10.0));
                            ui.add(egui::TextEdit::singleline(&mut self.new_mask).hint_text(lang.t("例如 255.255.255.255", "e.g. 255.255.255.255")).desired_width(100.0));
                        });
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(lang.t("目标网关 *", "Gateway *")).size(10.0));
                            ui.add(egui::TextEdit::singleline(&mut self.new_gateway).hint_text(lang.t("例如 10.39.165.114", "e.g. 10.39.165.114")).desired_width(100.0));
                        });
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(lang.t("路由别名", "Alias")).size(10.0));
                            ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text(lang.t("例如 公司测试网络", "e.g. Test Net")).desired_width(100.0));
                        });
                    });

                    ui.add_space(8.0);

                    // 精美的蓝紫色渐变添加按钮
                    let btn_text = lang.t(" 新 增 静 态 路 由 ", " ADD STATIC ROUTE ");
                    let add_button = egui::Button::new(
                        egui::RichText::new(btn_text)
                            .strong()
                            .color(egui::Color32::WHITE)
                    )
                    .fill(egui::Color32::from_rgb(13, 71, 161));

                    ui.vertical_centered_justified(|ui| {
                        if ui.add(add_button).clicked() {
                            let target = self.new_target.trim().to_string();
                            let mask = self.new_mask.trim().to_string();
                            let gateway = self.new_gateway.trim().to_string();
                            let mut name = self.new_name.trim().to_string();

                            if target.is_empty() || mask.is_empty() || gateway.is_empty() {
                                self.set_error(lang.t("必填项 (*) 不能为空", "Required fields (*) cannot be empty").to_string());
                                return;
                            }

                            if Ipv4Addr::from_str(&target).is_err() {
                                self.set_error(lang.t("目标 IP 地址格式不合法", "Invalid Target IP format").to_string());
                                return;
                            }
                            if Ipv4Addr::from_str(&mask).is_err() {
                                self.set_error(lang.t("子网掩码格式不合法", "Invalid Netmask format").to_string());
                                return;
                            }
                            if Ipv4Addr::from_str(&gateway).is_err() {
                                self.set_error(lang.t("网关地址格式不合法", "Invalid Gateway IP format").to_string());
                                return;
                            }

                            if name.is_empty() {
                                name = if lang == Language::Zh {
                                    format!("路由-{}", target)
                                } else {
                                    format!("Route-{}", target)
                                };
                            }

                            // 1. 检查目标 IP 是否已存在，防止重复添加
                            let exists = self.state.read().config.routes.iter().any(|r| r.target == target);
                            if exists {
                                let err_msg = if lang == Language::Zh {
                                    format!("目标 IP {} 已经存在配置列表中", target)
                                } else {
                                    format!("Target IP {} already exists in the configurations", target)
                                };
                                self.set_error(err_msg);
                                return;
                            }

                            // 2. 构造新路由项
                            let new_item = RouteItem {
                                id: format!("{}-{}", target.replace(".", "-"), chrono::Local::now().timestamp_millis()),
                                name,
                                target,
                                mask,
                                gateway,
                                enabled: false,
                                status: RouteStatus::Inactive,
                            };

                            // 3. 写入配置并保存
                            {
                                let mut state = self.state.write();
                                state.config.routes.push(new_item);
                                let _ = save_config(&state.config);
                            }

                            // 4. 清除临时输入并显示成功提示
                            self.new_target.clear();
                            self.new_name.clear();
                            self.new_gateway.clear();
                            self.new_mask = "255.255.255.255".to_string();

                            self.set_success(lang.t("静态路由配置已成功添加到本地！", "Static route config added successfully!").to_string());
                        }
                    });
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            // 流量统计累加与昨日/今日对比 (MB)

            // 计算环比波动 (今日与昨日相比)
            let (diff_text, diff_color) = if yesterday_traffic == 0.0 {
                if today_traffic == 0.0 {
                    ("0.0%".to_string(), egui::Color32::from_rgb(150, 150, 160)) // 灰色
                } else {
                    ("+100.0%".to_string(), egui::Color32::from_rgb(220, 80, 80)) // 红色
                }
            } else {
                let diff = (today_traffic - yesterday_traffic) / yesterday_traffic * 100.0;
                if diff > 0.0 {
                    (format!("+{:.1}%", diff), egui::Color32::from_rgb(220, 80, 80)) // 红色
                } else if diff < 0.0 {
                    (format!("{:.1}%", diff), egui::Color32::from_rgb(46, 125, 50)) // 绿色
                } else {
                    ("0.0%".to_string(), egui::Color32::from_rgb(150, 150, 160)) // 灰色
                }
            };

            // 绘制昨日/今日总流量与环比对比
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(lang.t("今日流量: ", "Today: ")).size(10.0).color(egui::Color32::from_rgb(160, 160, 170)));
                ui.label(egui::RichText::new(format_traffic(today_traffic)).size(10.5).strong().color(egui::Color32::WHITE));
                
                ui.add_space(6.0);
                ui.label(egui::RichText::new("|").size(10.0).color(egui::Color32::from_rgb(60, 60, 70)));
                ui.add_space(6.0);
                
                ui.label(egui::RichText::new(lang.t("昨日流量: ", "Yesterday: ")).size(10.0).color(egui::Color32::from_rgb(160, 160, 170)));
                ui.label(egui::RichText::new(format_traffic(yesterday_traffic)).size(10.5).strong().color(egui::Color32::WHITE));
                
                ui.add_space(6.0);
                ui.label(egui::RichText::new("|").size(10.0).color(egui::Color32::from_rgb(60, 60, 70)));
                ui.add_space(6.0);
                
                ui.label(egui::RichText::new(lang.t("环比对比: ", "Change: ")).size(10.0).color(egui::Color32::from_rgb(160, 160, 170)));
                ui.label(egui::RichText::new(diff_text).size(10.5).strong().color(diff_color));
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // 底部归属与联系信息
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(lang.t("产品归属：衢州御风科技有限公司", "Powered by: Quzhou Yufeng Technology Co., Ltd.")).size(10.0).color(egui::Color32::from_rgb(120, 120, 130)));
                ui.label(egui::RichText::new("|").size(10.0).color(egui::Color32::from_rgb(80, 80, 90)));
                ui.hyperlink_to(
                    egui::RichText::new("www.yftec.top").size(10.0).color(egui::Color32::from_rgb(0, 180, 216)),
                    "http://www.yftec.top"
                );
                ui.label(egui::RichText::new("|").size(10.0).color(egui::Color32::from_rgb(80, 80, 90)));
                ui.label(egui::RichText::new(lang.t("邮箱：admin@yftec.top", "Email: admin@yftec.top")).size(10.0).color(egui::Color32::from_rgb(120, 120, 130)));
            });
        });
    }
}

/// 载入 Windows 系统自带的微软雅黑或宋体，解决 egui 中文乱码（方块）问题，完美保障显示正常。
pub fn setup_chinese_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 常用 Windows 中文字体系统路径
    let font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",   // 微软雅黑
        "C:\\Windows\\Fonts\\msyh.ttf",    // 微软雅黑 (旧)
        "C:\\Windows\\Fonts\\simsun.ttc",   // 宋体
        "C:\\Windows\\Fonts\\simsun.ttf",    // 宋体 (旧)
    ];

    let mut loaded = false;
    for path in &font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "chinese_font".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            loaded = true;
            break;
        }
    }

    if loaded {
        // 设置为 Proportional 和 Monospace 字体族的首选
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            vec.insert(0, "chinese_font".to_owned());
        }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            vec.insert(0, "chinese_font".to_owned());
        }
        ctx.set_fonts(fonts);
    }
}

/// 辅助函数：当字符串过长时截断并以 "..." 结尾，防止 UI 被超长文本挤压变形。
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        s.chars().take(max_chars).collect::<String>() + "..."
    } else {
        s.to_string()
    }
}

/// 辅助函数：将 MB 流量智能转换为 MB 或 GB 单位，增强阅读体验。
fn format_traffic(mb: f64) -> String {
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{:.2} MB", mb)
    }
}

