# 🚀 Simpleroute v0.1.0 - 首次发布 (First Release)

`Simpleroute` 是一款专为 Windows 平台打造的高档、极轻量、高度稳定的静态路由托盘守护与流量监控管理工具。

### ✨ 核心功能亮点 (Highlights)
*   **🔄 路由自检与 1s 静默恢复**：后台每秒自动检测静态路由状态，若发生断网或插拔网线导致路由丢失，可在 1 秒内自动静默拉起恢复。
*   **📶 智能网卡流量监控**：秒级动态统计物理网卡上传/下载速率。未选中时智能优先监听活跃无线网卡，并提供 ComboBox 图形化网卡切换及防突变差分保护。
*   **🚨 自定义流量超额报警**：支持自由配置流量超额阈值，超出时自动触发系统气泡弹窗报警。
*   **🌐 Win32 原生中英多语言自适应**：原生探测 Windows 默认 UI 语言（UserDefaultUILanguage），自动切换中/英文 UI 及托盘气泡，动态注册微软雅黑字体彻底解决中文乱码。
*   **🔒 UAC 管理员特权与注入安全**：内置 Manifest 自动请求管理员特权，对 IP 掩码进行硬核 IPv4 校验以彻底杜绝 Shell 注入隐患。

### 📦 运行分发
- **`simpleroute.exe`**：无 WebView / Electron 等巨型依赖，常驻运行内存仅 `< 20MB`，直接双击运行即可。

---

*Licensed under the [Apache-2.0 License](LICENSE).*

---

### 🏢 Copyright & Support (版权与技术支持)
* **Powered By / 产品归属**: 衢州御风科技有限公司 (Quzhou Yufeng Technology Co., Ltd.)
* **Official Website / 官方网站**: [www.yftec.top](http://www.yftec.top)
* **Contact Email / 联系邮箱**: [admin@yftec.top](mailto:admin@yftec.top)
