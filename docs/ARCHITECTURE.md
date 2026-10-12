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
src/codex.rs                # 本机 Codex app-server 额度采集、窗口解析与错误状态
src/claude.rs               # Claude CLI get_usage 协议、账户绑定与额度状态
src/quota.rs                # 共享额度窗口与百分比/重置时间格式
src/child_process.rs        # Windows Job Object 子进程生命周期
src/glow.rs                 # 连续高斯圆环光晕
src/platform_windows.rs     # 小范围 Windows 原生 API 封装
src/placement.rs            # 可测试的多屏工作区坐标算法
src/state.rs                # 模式、位置和启用服务的 JSON 持久化
scripts/capture-ui.ps1      # 隔离配置的原生界面快照检查
scripts/check-position.ps1  # 隔离配置的两种模式共享位置检查
scripts/check-startup.ps1   # 弹出／常驻启动显隐检查
scripts/check-entry-session.ps1 # 正式入口形态与位置的退出重启检查
```

## 窗口

- **MiniWindow / CapsuleWindow**：首次启动显示 Mini，之后恢复退出时的形态；左键头像即时展开／收起，透明背景、无边框、置顶。二者独立保存位置，超过 7px 才开始原生拖动。
- **DetailWindow**：点选某项时弹出，显示多个额度说明及真实接入的预留状态。
- **SettingsWindow**：常规／服务管理／外观／行为／关于，深色 Fluent 下拉框和滑杆。
- **EntryMenuWindow**：Mini 和胶囊共用的 128×84px 深色图标菜单窗口，只有设置和退出两项。右键位置转换为屏幕坐标后限制在工作区内，失焦与 Esc 关闭；隐藏时不绘制。使用独立窗口避免小尺寸入口裁切菜单。
- **WacuTray**：仅用于 Debug 旧模式检查，正式版不创建。

## 使用状态

`entry_view` 独立于旧版 `mode`，取值 `mini` / `capsule`，旧配置缺失时默认 Mini。切换开始时记录目标形态，避免交接动画期间两窗同时可见导致退出状态歧义；退出事件循环后再次保存形态和可见窗口的正常坐标。启动直接显示保存的入口，不先显示 Mini 再展开。设置、详情、菜单不会改变该形态，设置常驻选项在正式入口模式下也不切换窗口显隐。恢复的胶囊不会沿用旧托盘模式的三秒自动隐藏。

用户设置文件保存在 `%APPDATA%/Wacu/preferences.json`，修改时使用先写临时文件再原子替换的策略。仓库中不保存 API Key、Token 或用户资料。

贴边隐藏和左右位置新增字段使用 Serde 默认值兼容旧配置。胶囊隐藏时仅移动原生窗口，保留 6 个逻辑像素的可见边缘；配置中只记录恢复后的正常位置。检测间隔 200ms，位置落盘最多每秒一次。边缘恢复后保留 800ms，让鼠标穿过桌面间隙。

开机启动状态从 `HKCU/Software/Microsoft/Windows/CurrentVersion/Run` 的 Wacu 项读取，用户切换时才写入或删除，读取失败按关闭显示；路径用引号包裹。未操作该开关时不会修改 Windows 启动项。

## 性能守则

- 不增加额外模型调用获取用量。
- 定时请求 2–5 分钟起步，断网/限流自动退避；倒计时不触发网络请求。
- 动画只在交互或数据变化时执行，不以 60 FPS 轮询刷新。
- 视觉胶囊与后续服务采集器解耦，采集器仅输出统一 UsageSnapshot。
- Release 实机验证 CPU、内存、老显卡渲染器兼容与多屏 DPI。

## Codex 真实额度采集

`src/codex.rs` 发现 PATH / npm 原生二进制 / Codex 桌面应用的 `codex.exe`，也支持 `WACU_CODEX_PATH`。通过隐藏的 stdio 子进程依次发送 `initialize`、`initialized`、`account/read`、`account/rateLimits/read`。不创建 Codex 会话或推理任务，不直接访问认证文件；子进程沿用本机 Codex 登录与 `CODEX_HOME`。

优先解析 `rateLimitsByLimitId`，保留全部额度桶；`rateLimits` 仅补充旧版主桶。圆环只使用 `codex` 桶（或旧版无 ID 的主桶），不会把另一个模型的额度当作 Codex 主额度。`usedPercent` 空值保持未知；窗口长度与重置时间均允许缺失，百分比限制在 0–100。

采集工作线程通过 channel 返回不含身份信息的 `Snapshot`；UI 每秒检查本地结果，不在 UI 线程执行网络请求。正常刷新间隔 120 秒，错误退避 240 / 480 / 960 / 1800 秒；服务停用暂停自动请求，手动刷新合并进行中的请求。每轮 30 秒总超时，请求完成或失败均终止并回收子进程。

UI 线程保存最近快照与状态，刷新额度时只更新对应服务行及打开的详情，不改变窗口几何、偏好或选中状态。临时错误保留旧快照并注明；退出登录、缺少 CLI 或不支持的账户清空旧额度。快照只驻留内存，仓库与偏好文件不存账户、Token、额度缓存。Windows Job Object 在 Wacu 退出时自动回收仍在查询的子进程。

Debug 原生界面 QA 默认禁用网络采集；`scripts/check-codex.ps1` 显式启用只读的真实额度验收，检查圆环、快览、完整详情和服务管理的数据同步。截图保存在忽略的 `target/`；不归档私人额度到仓库。

接口依据：[OpenAI 官方 App Server 文档](https://learn.chatgpt.com/docs/app-server#6-rate-limits-chatgpt)。

## Claude Code 本地额度采集

发现 PATH 中的原生 `claude.exe`、npm 原生二进制、用户 `.local/bin`，支持 `WACU_CLAUDE_PATH`。通过隐藏 CLI 的 `-p --input-format stream-json --output-format stream-json --verbose` 发送单个 `get_usage` 控制请求；不发送模型提示词。启用 safe-mode、禁用 hooks/工具/MCP 与会话持久化，工作目录为 `%LOCALAPPDATA%/Wacu/claude-engine`。认证、代理与 Token 刷新交由 CLI 自身处理，Wacu 不访问凭据文件。

查询前后执行 `auth status --json`，要求 claude.ai/firstParty 登录及可核对的邮箱、组织。账户原始标识仅驻留内存，诊断快照不序列化身份；详情只显示脱敏邮箱和套餐。前后身份不同丢弃本轮结果；相对上次成功快照已切换账号且查询失败时清空旧数据。CLI 来源不会代表 Desktop 已核对，同账号的共享额度无需相加。

解析 5 小时、每周及模型专属窗口，保留空值为未知，`utilization`/`percent` 均为 0–100。`limits[]` 的模型周额度只接受 weekly 组且没有 surface 限制的项目，并覆盖对应旧模型窗口。主环只选择共享 5 小时/每周窗口。额外用量仅显示比例，不推定金额单位。

后台独立工作线程仅在本机 Code 会话工作时按 300 秒间隔刷新；空闲或等待输入／授权时暂停。失败退避至 1800 秒，限流至少冷却 900 秒；手动刷新有 60 秒冷却，也不绕过限流。40 秒总超时覆盖账户核对与查询，结束后回收子进程，Wacu 退出由共享 Windows Job Object 保证清理。UI 默认未知，不以旧演示百分比填补查询失败。

`claude_activity.rs` 接收官方生命周期 hook 的最少字段，用匿名哈希区分会话／子任务，保存工作状态与原生父进程 PID、创建时间。锁文件串行化同会话并发事件，临时文件原子替换；不保存提示词、工具参数、返回内容或账号。监测线程每两秒聚合活动会话，并验证进程仍存活且创建时间相同，避免退出后残留状态及 PID 重用。Desktop 与 CLI 的状态共同决定是否刷新，但额度仍来自经核对的 CLI 账号。

`claude_schedule.rs` 统一限制自动间隔、手动冷却及错误退避，恢复工作不重置间隔。额度子进程继承 `WACU_QUOTA_READER`，同时保留禁用 hooks 的设置；自身读取不会反过来激活状态检测。连接工具以备份、检查原文件未并发改变、合并专属 hook、原子替换的顺序更新 Claude 设置。可移除专属 hook，保留其他配置。支持 native exec-form hooks，本机终端／Desktop Code 可共用；未覆盖远程和非 Code 模式。

本地协议参考 [Agent HUD](https://github.com/jazzenchen/agent-hud-open/blob/main/Sources/AgentHUDCore/Providers/Claude/ClaudeEngineUsage.swift)，不是公开稳定 API。另已审查 [CC Switch](https://github.com/farion1231/cc-switch/blob/main/src-tauri/src/services/subscription.rs) 的直接 OAuth HTTP 实现；Wacu 未采用其凭据读取方式。目前真实 `get_usage` 响应尚未验收，默认 QA 使用离线样例并明确标注，只有 `check-claude.ps1 -Live` 显式启用联网验收。

## 后续统一用量结构

```rust
struct UsageSnapshot {
    service_id: String,
    status: UsageStatus, // Ok, Stale, Unauthorized, Unsupported, Error
    collected_at: std::time::SystemTime,
    windows: Vec<QuotaWindow>,
}
```

真实数据显示“剩余百分比”之前必须明确来源和窗口；不可把模拟数据冒充真实配额。
