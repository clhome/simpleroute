# Simpleroute - Minimalist Windows Static Route Tray Daemon

[![Platform](https://img.shields.io/badge/platform-Windows-blue.svg)](https://microsoft.com/windows)
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org)
[![UI Framework](https://img.shields.io/badge/UI-egui-green.svg)](https://github.com/emilk/egui)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`Simpleroute` is an **elegant, ultra-lightweight, and highly stable** static route tray manager built exclusively for the Windows platform. Written in Rust and utilizing the GPU-accelerated `egui` framework without any heavy browser WebView dependency, the application runs persistently in the Windows system tray.

It is designed to seamlessly assist developers, network engineers, and power users who frequently switch between corporate intranets, public internets, Virtual Private Networks (VPNs), or multi-NIC hybrid environments, offering one-click routing toggle, active guard, and real-time traffic monitoring.

---

## ✨ Key Features

*   **⚡ Minimalist System Tray & Quick Control**
    *   Sleek residency in the Windows taskbar tray with a highly responsive right-click context menu.
    *   Enable or disable specific routing rules with a single click directly from the tray menu.
    *   Command shortcuts including "Redetect Network", "Open UI Panel", "Minimize to Tray", and "Safe Exit".
*   **🎨 GPU-Accelerated Dark-Themed GUI Panel**
    *   Built on the pure-Rust `egui`/`eframe` GUI engine, eliminating heavy Chromium WebView or Electron runtimes.
    *   **Ultra-lightweight footprint**: Memory consumption is typically `< 20MB`, with CPU usage near `0%`.
    *   Dynamically registers and loads premium Windows Microsoft YaHei vector fonts to **prevent Chinese text rendering issues (box glitches)**.
*   **📊 Real-Time Network Traffic Cards**
    *   Visually premium card layout displaying instantaneous **Download (📥) and Upload (📤) speeds (MB/s)** calculated with sub-second accuracy.
*   **📶 Intelligent NIC Traffic Selector with Hot-Switch Protection**
    *   **Wi-Fi Priority (Auto)**: If no specific network card is locked, the program automatically queries the Windows IP Helper API to select and monitor the **first active wireless adapter** (`IF_TYPE_IEEE80211` = 71).
    *   **Automatic Ethernet Fallback**: If no active Wi-Fi is detected, it falls back to the first active physical wired network adapter, and lastly aggregates all non-loopback adapter flows as a safety net.
    *   **Adapter Visual Select**: Features an `egui::ComboBox` dropdown list, rendering available network cards along with intuitive `📶` (Wireless) or `🔌` (Wired) icons and connection status.
    *   **Differential Burst Protection**: Instantly resets traffic accumulation baselines when switching network cards to prevent virtual speed spikes.
*   **🚨 Over-Limit Traffic Bubble Notification Alarm**
    *   Users can easily enable/disable traffic warnings and configure customized bandwidth thresholds (e.g., 10.0 MB/s).
    *   When the instant download or upload speed exceeds the limit, it calls the native Windows WinRT Notification engine to pop up **bubble alerts** warning of bandwidth anomalies.
*   **🔄 Automatic Scanning & Merging of Existing Routes**
    *   **Silent Sync**: During startup, the program quietly runs a hidden `route print -4` parser in the background.
    *   **High-Precision Filter**: Excludes loopbacks, direct local subnets, multicast (`> 224.0.0.0`), and default gateways (`0.0.0.0`), identifying existing manually configured static routes and merging them into the local JSON config file with a green "Active" status.
*   **🛡️ 1-Second High-Frequency Health Guard**
    *   Runs a dedicated background daemon checking the validity of marked "Enabled" routes every second.
    *   If a route is lost due to network state changes (e.g., pulling out Ethernet, switching Wi-Fi, connecting VPN), the daemon **automatically rebuilds it in under 1 second**.
*   **🔒 Automated Privilege Escalation & Input Sanitization**
    *   **UAC Manifest Integration**: Embedded Windows application manifest forces administrative privilege requests on startup, preventing execution failures.
    *   **Command Injection Defense**: Enforces strict regex validation on IPv4 addresses, subnet masks, and gateways, entirely blocking arbitrary shell command execution risks.

---

## 🏗️ Architecture & Operating Principles

### 1. Software Structure

The program architecture decouples the main rendering thread, the system tray interaction thread, and the background daemon threads. Data sharing is facilitated through high-performance `parking_lot::RwLock` reader-writer locks to ensure thread safety without deadlocks.

```
Simpleroute (Windows Native Executable)
 ├─ Embedded Manifest -> UAC administrative permission prompt
 ├─ Graphical Main Thread (Rust + egui) -> GPU hardware-accelerated GUI panel
 ├─ System Tray Thread (tray-icon) -> Persistent residency & right-click interactions
 ├─ Background Daemon Thread (1s interval)
 │   ├─ Traffic Calculation -> Calls GetIfTable2 API to extract adapter octets
 │   ├─ Route Validity Checker -> Background validation & automatic recovery
 │   └─ Toast Dispatcher -> Triggers notifications using Windows WinRT APIs
 └─ Local Configuration -> %APPDATA%\simpleroute\config.json (serde_json)
```

### 2. Core Win32 / WinRTS System APIs
*   **Adapter Bandwidth Tracking**: Utilizes Win32 `GetIfTable2` to poll the interface octets (`InOctets` and `OutOctets`) and free tables cleanly using `FreeMibTable`.
*   **Route Control Layer**: Directly issues parameter-safe Win32 `Command::new("route")` system calls to alter routing entries, bypassing generic cmd interpreters.

---

## 🛠️ Compilation & Building

### 1. Prerequisites
Since this software is a native privilege manager for Windows, it must be compiled on a **Windows Operating System**.

1.  **Install Rust Compiler Toolchain**
    *   Download and run [RustUp](https://rustup.rs/).
    *   Ensure your active toolchain is `x86_64-pc-windows-msvc` and the Rust compiler version $\ge 1.70$.
2.  **Install C++ MSVC Build Tools**
    *   If your system lacks a C++ compiler, ensure you check the Visual Studio Build Tools option during RustUp's environment diagnostics.

### 2. Compile Commands

Open your terminal (PowerShell or Cmd) at the project root directory:

*   **Build Debug Target (With symbol and debugging logs)**
    ```powershell
    cargo build
    ```
    The generated executable is located at `target/debug/simpleroute.exe`.

*   **Build Release Target (Fully optimized size & performance)**
    ```powershell
    cargo build --release
    ```
    The generated executable is located at `target/release/simpleroute.exe`. This builds a trimmed, lightweight, and high-performance binary ideal for persistent distribution.

*   **Run Directly**
    ```powershell
    cargo run
    ```

---

## 📖 Usage & Configuration Instructions

### 1. App Startup & Privileges
Double-click `simpleroute.exe`. Windows User Account Control (UAC) will prompt for administrative access. Please select **"Yes"**.
*   *Note: Modifying static routes requires administrative access on Windows; the tool will request this automatically to prevent silent API failures.*

### 2. Local Configuration JSON
All configuration data is saved strictly on your local filesystem:
*   **Standard Directory**: `%APPDATA%\simpleroute\config.json` (i.e. `C:\Users\<Username>\AppData\Roaming\simpleroute\config.json`).
*   *Note: If AppData is inaccessible, it gracefully defaults to creating `config.json` in the same directory as the executable.*

#### Config JSON Example:
```json
{
  "routes": [
    {
      "id": "10-0-0-0-1716357890000",
      "name": "Corporate Intranet",
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

### 3. Quick Operation Guide
1.  **Add New Route**: At the bottom of the graphical panel, enter "Target IP", "Subnet Mask", "Gateway", and a friendly alias, then click **"Add Static Route"**.
2.  **Toggle Routes**: Use the toggles next to each route item on the GUI, or right-click the system tray icon to enable/disable rules instantly.
3.  **Lock Adaptor**: Select the target wired/wireless card from the dropdown under "Network Traffic Card" to lock the speed calculations. Select `💡 Wi-Fi Priority (Auto)` to restore automatic priority scanning.
4.  **Configure Alarm**: Toggle the alarm status on the traffic card, type in your warning speed limit (e.g., `10.0`), and click anywhere outside to save.

---

## 📜 License

This project is licensed under the [Apache License 2.0](LICENSE). You are free to modify, distribute, and integrate it into your workflows.
