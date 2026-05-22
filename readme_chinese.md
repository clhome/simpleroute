# Simpleroute - 极简 Windows 静态路由托盘守护工具

简体中文 | [English](readme.md)

[![Platform](https://img.shields.io/badge/platform-Windows-blue.svg)](https://microsoft.com/windows)
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org)
[![UI Framework](https://img.shields.io/badge/UI-egui-green.svg)](https://github.com/emilk/egui)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`Simpleroute` 是一个专为 Windows 平台打造的**高档、极轻量、高度稳定**的静态路由托盘守护管理工具。该程序常驻于 Windows 任务栏右下角托盘，使用 Rust 语言配合纯 GPU 硬件加速的 `egui` 无 WebView 渲染引擎开发。

它能完美帮助经常需要在公司内网、外网、虚拟专用网 (VPN) 或多网卡混合开发网络之间频繁切换的开发者与运维人员，实现静态路由规则的一键快捷切换、状态守护与实时流量监测。

---

## ✨ 核心特性 (Key Features)

*   **⚡ 极简托盘常驻与快捷控制**
    *   静默常驻 Windows 右下角任务栏托盘，提供极富科技感的右键菜单。
    *   支持在托盘菜单中一键快捷开启/关闭特定的路由规则。
    *   提供“重新检测网络”、“打开管理主界面”、“最小化至托盘”及“安全退出”等常备指令。
*   **🎨 GPU 硬件加速的暗黑系 UI 界面**
    *   基于纯 Rust 开发的 `egui`/`eframe` 渲染引擎，免除 Electron 或 Tauri 重型的浏览器 WebView 依赖。
    *   **极致轻量**：安装后无需携带巨型运行时，内存占用极其低（通常 `< 20MB`），CPU 占用接近于 `0%`。
    *   内置微软雅黑高级矢量中文字体动态注册加载器，**彻底杜绝中文乱码（方块）**，观感精美雅致。
*   **📊 实时网速流量统计卡片**
    *   提供大字号、精美的圆角网速卡片，秒级动态计算并展示系统的瞬时**下载 (📥 Download) 与上传 (📤 Upload) 速率 (MB/s)**。
*   **📶 智能网卡流量自选与智能切换保护**
    *   **无线网卡自动优先**：如果用户未指定监听网卡，系统通过 Windows IP Helper API 智能搜索并优先显示**第一个处于连接活跃状态的无线网卡**。
    *   **智能回退机制**：若无活跃无线网络，自动退水回退到第一个活跃的有线物理网卡，直至兜底累加；
    *   **网卡图形化自选**：右上角提供 `egui::ComboBox` 下拉网卡列表，以精美的 `📶` (无线) 和 `🔌` (有线) 图标展示系统所有网卡及已连接状态，供用户随时锁定监听。
    *   **差分防突变保护**：网卡切换瞬间自动重置流量统计基准，彻底消除了由于字节差分爆发出现“网速瞬间飙升至数万MB/s”的 bug。
*   **🚨 流量超限额气泡弹窗报警**
    *   支持用户自定义开启/关闭流量超额弹窗报警，并可自由配置报警阈值上限（如 10.0 MB/s）。
    *   当瞬时下载或上传网速达到设置的超限值时，自动调用 Windows WinRT 系统通知引擎触发**气泡弹窗报警**，及时警告网络异常。
*   **🔄 已有静态路由扫描与自动同步**
    *   **静默同步**：软件每次启动时，会在后台静默运行隐藏的 `route print -4` 路由扫描器。
    *   **高精度去重解析**：通过严格的直连网段、本地回环、默认网关、组播等规则过滤，精准识别出当前系统上手工配置的已有静态路由，并一键去重同步到本地 JSON 配置文件中，赋予其绿色“已生效”标签。
*   **🛡️ 1 秒级高频路由健康监视与自动重建**
    *   后台开启高频路由检测线程，每秒校验列表中标记为“已开启”的静态路由状态。
    *   一旦检测到因拔插网线、切换 WiFi、VPN 拨号等网络变化导致路由丢失，后台线程会自动在 **1 秒内实现静默拉起重建**，保障网络畅通无阻。
*   **🔒 管理员特权与严密输入安全机制**
    *   **强制 UAC 请求**：内部嵌入 Windows 资源清单 Manifest，双击程序启动时自动请求系统管理员特权以保证静态路由修改成功。
    *   **命令注入防御**：对用户输入的 IP、掩码、网关进行严格的 IPv4 正则校验，拒绝直接拼接执行，彻底杜绝 Shell 命令注入安全隐患。

---

## 🏗️ 技术架构与工作原理

### 1. 软件架构设计

程序分为主图形线程、系统托盘线程、后台路由健康守护与网速轮询线程，它们通过 `parking_lot::RwLock` 实现数据的高性能读写互斥，架构极度紧凑。

```
Simpleroute (Windows 应用程序)
 ├─ 嵌入式 Windows Manifest -> 强制 UAC 管理员权限请求
 ├─ 主界面 GUI 线程 (Rust + egui) -> 纯 GPU 硬件加速渲染，微软雅黑字体动态注册
 ├─ 托盘交互线程 (tray-icon) -> 响应托盘右键菜单，实现无感隐藏与呼出
 ├─ 后台守护线程 (1s 周期轮询)
 │   ├─ 流量统计计算 -> 调用 GetIfTable2 获取物理网卡数据计算瞬时速率
 │   ├─ 路由状态监控 -> 后台比对路由有效性，网络变化 1 秒内自动重拉
 │   └─ 气泡报警触发 -> 通过 WinRT Notification 发送流量报警通知
 └─ 本地数据持久化 -> %APPDATA%\simpleroute\config.json (基于 serde_json)
```

### 2. 关键系统接口
*   **网络数据流统计**：调用 Windows 平台自带的 `GetIfTable2` 并释放 MibTable，提取各网卡的 `InOctets` 和 `OutOctets` 进行时间差分换算。
*   **静态路由管理**：直接以无窗口隐藏模式调用 Windows 原生 `route.exe` (使用 `Command::new("route")` 传参模式，防注入且兼容性最佳)。

---

## 🛠️ 编译与构建 (Compilation & Building)

### 1. 前期准备 (Prerequisites)
由于本软件是 Windows 平台原生特权工具，必须在 **Windows 操作系统** 下进行编译。

1.  **安装 Rust 编译器工具链**
    *   下载并安装 [RustUp](https://rustup.rs/)。
    *   确保安装的工具链为 `x86_64-pc-windows-msvc`，且 Rust 编译器版本 $\ge 1.70$。
2.  **安装 C++ MSVC 构建环境**
    *   如果您的电脑尚未配置 C++ 编译环境，请在安装 RustUp 时按照提示下载并安装 Visual Studio 构建工具 (Build Tools)。

### 2. 构建步骤 (Building)

在项目根目录下打开终端 (PowerShell 或 Cmd)，执行以下命令：

*   **编译 Debug 版本 (包含调试信息)**
    ```powershell
    cargo build
    ```
    编译生成的可执行文件位于：`target/debug/simpleroute.exe`。

*   **编译 Release 生产版本 (极致体积与性能优化)**
    ```powershell
    cargo build --release
    ```
    编译生成的可执行文件位于：`target/release/simpleroute.exe`。该版本经过了死代码裁剪与链接优化，运行速度最快，体积极小，适合分发和长期常驻。

*   **一键运行**
    ```powershell
    cargo run
    ```

---

## 📖 使用与配置说明 (Usage Instructions)

### 1. 软件启动与权限请求
双击 `simpleroute.exe`，Windows 系统会弹出 UAC 用户账户控制气泡，提示“该程序将以管理员权限运行”，请点击 **“是”**。
*   *注：修改系统静态路由表属于敏感系统特权，必须在管理员特权下才能成功执行，本工具会自动请求这一权限，不会产生静默失败。*

### 2. 本地配置文件说明
软件采用完全本地化的数据持久化方案，所有的配置数据均保存在以下 JSON 配置文件中：
*   **标准位置**：`%APPDATA%\simpleroute\config.json`（即 `C:\Users\<您的用户名>\AppData\Roaming\simpleroute\config.json`）。
*   *注：如果该目录不可写，程序会自动在同级目录下创建并读写 `config.json`，确保无损运行。*

#### 配置文件 JSON 示例：
```json
{
  "routes": [
    {
      "id": "10-0-0-0-1716357890000",
      "name": "公司测试网络",
      "target": "10.0.0.0",
      "mask": "255.0.0.0",
      "gateway": "192.168.1.1",
      "enabled": true,
      "status": "Active"
    }
  ],
  "enable_alarm": true,
  "alarm_limit_mb": 15.0,
  "selected_interface_name": null
}
```

### 3. 常规交互指南
1.  **新增路由**：在管理主界面的底部表单中，输入“目标IP”、“子网掩码”、“目标网关”及“路由别名”，点击 **“新增静态路由”** 即可保存。
2.  **快捷开关**：在主界面路由项右侧，或者直接在右下角托盘图标上右键，点击路由名称左侧的开关，即可实现毫秒级启用/禁用。
3.  **网卡选择**：如需锁定统计特定网卡的流量，展开仪表盘下方的下拉菜单，点选有线或无线网卡；点选 `💡 默认无线网卡优先` 可自动复原。
4.  **流量报警**：在此卡片下方开启报警开关，输入您预设的限速数值（例如 `10.0`），点击其他空白区域即保存。

---

## 📜 许可证 (License)

本项目采用 [Apache-2.0 许可证](LICENSE) 开源。您可以自由进行修改、分发与二次开发。

---

## 🏢 版权与产品支持 (Copyright & Support)

* **产品归属**：衢州御风科技有限公司
* **官方网站**：[www.yftec.top](http://www.yftec.top)
* **联系邮箱**：[admin@yftec.top](mailto:admin@yftec.top)
