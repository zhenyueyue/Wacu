# Wacu · Rust + Slint 原生架构 v0.2

## 确定路线

- **Rust**：应用主进程、状态、配置、数据采集和任务调度。
- **Slint**：Mini、圆角胶囊、环形进度、详情、设置与图标菜单的 UI 和动画；SystemTrayIcon 仅保留在调试路径。
- **Win32 API**：桌面工作区计算、窗口的 ToolWindow 属性、窗口锚点定位、拖动、前台激活、当前用户 Run 启动项与系统浏览器入口。
- **没有 WebView、Node.js、Tauri、React、Chromium**：对低配电脑友好、独立运行。

## 文件结构

```
Cargo.toml                  # 唯一 Rust 工程
build.rs                    # Slint UI 编译与 Windows 图标资源嵌入
ui/wacu.slint               # Mini、胶囊、详情、设置、图标菜单
assets/app.ico              # EXE 多尺寸品牌图标
assets/app-icon.png         # 窗口和托盘品牌图标
assets/icons/               # 内嵌 SVG 线条图标与服务标记
src/main.rs                 # UI 状态协调、事件和生命周期
src/glow.rs                 # 连续高斯圆环光晕
src/platform_windows.rs     # 小范围 Windows 原生 API 封装
src/placement.rs            # 可测试的多屏工作区坐标算法
src/state.rs                # 模式、位置和启用服务的 JSON 持久化
scripts/capture-ui.ps1      # 隔离配置的原生界面快照检查
scripts/check-position.ps1  # 隔离配置的两种模式共享位置检查
scripts/check-startup.ps1   # 弹出／常驻启动显隐检查
```

## 窗口

- **MiniWindow / CapsuleWindow**：正式版启动显示 Mini，左键头像即时展开／收起；透明背景、无边框、置顶。二者独立保存位置，超过 7px 才开始原生拖动。
- **DetailWindow**：点选某项时弹出，显示多个额度说明及真实接入的预留状态。
- **SettingsWindow**：常规／服务管理／外观／行为／关于，深色 Fluent 下拉框和滑杆。
- **EntryMenuWindow**：Mini 和胶囊共用的 128×84px 深色图标菜单窗口，只有设置和退出两项。右键位置转换为屏幕坐标后限制在工作区内，失焦与 Esc 关闭；隐藏时不绘制。使用独立窗口避免小尺寸入口裁切菜单。
- **WacuTray**：仅用于 Debug 旧模式检查，正式版不创建。

## 使用状态

用户设置文件保存在 `%APPDATA%/Wacu/preferences.json`，修改时使用先写临时文件再原子替换的策略。仓库中不保存 API Key、Token 或用户资料。

贴边隐藏和左右位置新增字段使用 Serde 默认值兼容旧配置。胶囊隐藏时仅移动原生窗口，保留 6 个逻辑像素的可见边缘；配置中只记录恢复后的正常位置。检测间隔 200ms，位置落盘最多每秒一次。边缘恢复后保留 800ms，让鼠标穿过桌面间隙。

开机启动状态从 `HKCU/Software/Microsoft/Windows/CurrentVersion/Run` 的 Wacu 项读取，用户切换时才写入或删除，读取失败按关闭显示；路径用引号包裹。未操作该开关时不会修改 Windows 启动项。

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
