# Wacu · Rust + Slint 原生架构 v0.2

## 确定路线

- **Rust**：应用主进程、状态、配置、数据采集和任务调度。
- **Slint**：圆角胶囊、环形进度、详情和设置等全部 UI 与动画；单独的 SystemTrayIcon 元素。
- **Win32 API**：桌面工作区计算、窗口的 ToolWindow 属性、窗口锚点定位、拖动与前台激活状态。
- **没有 WebView、Node.js、Tauri、React、Chromium**：对低配电脑友好、独立运行。

## 文件结构

```
Cargo.toml                  # 唯一 Rust 工程
build.rs                    # Slint UI 编译
ui/wacu.slint               # 胶囊、详情、设置、系统托盘
assets/tray.svg             # 内嵌品牌图标
src/main.rs                 # UI 状态协调、事件和生命周期
src/platform_windows.rs     # 小范围 Windows 原生 API 封装
src/placement.rs            # 可测试的多屏工作区坐标算法
src/state.rs                # 模式、位置和启用服务的 JSON 持久化
```

## 窗口

- **CapsuleWindow**：透明背景、无边框、置顶、圆角胶囊。默认仅托盘，不启动就显示；常驻模式持久化位置。
- **DetailWindow**：点选某项时弹出，显示多个额度说明及真实接入的预留状态。
- **SettingsWindow**：模式和服务管理。
- **WacuTray**：Slint 自带 Windows Shell 系统托盘，保证隐藏窗口时后台事件循环仍存活。

## 使用状态

用户设置文件保存在 `%APPDATA%/Wacu/preferences.json`，修改时使用先写临时文件再原子替换的策略。仓库中不保存 API Key、Token 或用户资料。

## 性能守则

- 不增加额外模型调用获取用量。
- 定时请求 2–5 分钟起步，断网/限流自动退避；倒计时不触发网络请求。
- 动画只在交互或数据变化时执行，不以 60 FPS 轮询刷新。
- 视觉胶囊与后续服务采集器解耦，采集器仅输出统一 UsageSnapshot。
- Release 实机验证 CPU、内存、老显卡渲染器兼容与多屏 DPI。

## 下阶段统一用量结构

```rust
struct UsageSnapshot {
    service_id: String,
    status: UsageStatus, // Ok, Stale, Unauthorized, Unsupported, Error
    collected_at: std::time::SystemTime,
    windows: Vec<QuotaWindow>,
}
```

真实数据显示“剩余百分比”之前必须明确来源和窗口；不可把模拟数据冒充真实配额。
