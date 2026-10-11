# Wacu

> A little watcher for your AI. 静静守望，每一份余量。

Windows 轻量 AI 用量守望工具。**现已完全重写为 Rust + Slint + Windows API**；运行时不使用 React、Tauri、Electron 或 WebView2。

## 界面

- **桌面 Mini / 纵向胶囊为正式入口**：首次启动显示约 66×66px 圆形 Mini；之后启动恢复退出时的形态与各自位置。左键 Mini 立即展开纵向胶囊，左键胶囊顶部头像立即收回 Mini，不使用系统托盘图标。
- 已移除双击操作和单双击等待。右键 Mini 或胶囊弹出紧凑深色圆角菜单：齿轮图标“设置”、电源图标“退出”，无图标底块，轻柔阴影和灰色悬停高亮；菜单外点击或 Esc 可取消。鼠标移动超过 7px 才开始原生拖动，拖动不会误触切换。查看[菜单预览](docs/screenshots/context-menu-hover.png)。
- EXE 与原生窗口统一使用品牌 Logo；程序内嵌 16–256px 多尺寸 Windows 图标。
- 胶囊顶部显示品牌 Logo，下方排列 Claude Code、Codex、WorkBuddy，以双眼角色、带背景轨道的进度环和已用百分比显示演示用量；**已移除底部设置齿轮及其空白**。默认标准档主体宽 72px、圆环 56px；外观页可选择紧凑／标准／宽松三档（窗口宽分别为 88／96／104px）并记住偏好，旧标准升级后自动对应新的紧凑档。
- 详情与设置采用紧凑排版：快览 296×422px、完整详情 296×612px、设置 460×600px；正文 13–14px、百分比 32px，使用石墨色渐变、圆角控件和柔和阴影。
- Mini 和完整胶囊**分别保存自己的位置**（`mini_position` 与 `saved_position`）：Mini 默认停在屏幕右下角，胶囊首次展开默认停在右侧中间。两者可分别拖动，展开/收起后各自回到原位置，不再根据另一窗口坐标重新定位。设置 → 行为可以单独恢复 Mini 或胶囊的默认位置。
- Mini ↔ 胶囊采用“圆环化线 → 分路汇入 Logo → 沿边缘卷入额度环”：Mini 的服务配色弧段化成短线，途中沿上下分开的轻弧路线运行，在 Logo 周围重新成环，再由同一条路径沿胶囊边缘下行、弯曲卷成各个额度环；途中不淡出换线。收起沿同一时间轴反向归拢。路线、弧段数、错位节奏和浮窗高度随当前启用的 0／1／2／3 项服务变化，保留原有颜色与用量。
- 两个入口始终留在各自保存位置，彩线由固定的透明绘制层呈现；展开约 1.08 秒、收起约 0.93 秒。Logo 和 Mini 同位时省略跨位置阶段，展开 630ms、收起 480ms。左键松开即启动，快速反向从当前进度继续；关闭动效后即时交接，静止后无帧循环。[三个额度环的异位实机预览（实际速度）](docs/screenshots/entry-logo-flow-count-3.gif)。
- 所有界面原生窗口在创建前就设置隐藏所有者、排除任务栏和默认不激活，避免切换时先出现任务栏按钮再移除；菜单与设置仍按需主动激活。
- 进度环使用 Slint 原生路径与圆头端点；详情保留短时淡入淡出，服务仍可显示选中高亮。
- 点击服务先弹出窄版用量快览，再展开带示意趋势图的完整详情。
- 深色分栏设置保留开机启动、服务管理、大小、动画、透明度和左右停靠位置。开机启动使用当前用户 Windows Run 注册表项，默认不启用；Mini 不启用旧版托盘弹出/贴边隐藏行为。
- 详细面板提供今日／7天／30天／全部筛选、24 根示意柱和官网入口。开源／反馈／帮助／检查更新按钮使用系统默认浏览器；当前语言仅提供简体中文，更新入口查看 GitHub Releases，不自动安装。
- **重要：目前全部百分比均为演示数据，尚未接入实际账户或 API。**

## 开发与运行

支持 Windows x64，安装 Rust MSVC 工具链和带 Windows SDK 的 Visual Studio C++ Build Tools（SDK 资源编译器用于内嵌程序图标）。

```powershell
cargo run
```

运行测试：

```powershell
cargo test
```

入口动画原生帧与交互验收（三档大小、0／1／2／3 项服务、同位与异位交接、快速反向与关闭动画；使用隔离配置）：

```powershell
cargo build
./scripts/check-entry-motion.ps1
./scripts/check-entry-handoff.ps1 -Archive # 同位与两个方向的异位交接、服务开关组合
python ./scripts/check-flow-continuity.py # 比较卷入末帧与静态额度环的原生像素
./scripts/check-entry-session.ps1 # 实际退出重启，检查窗口形态及独立位置恢复
python ./scripts/render-entry-handoff.py # 用归档的原生帧和桌面坐标生成 GIF
```

生成优化后的 Windows 可执行程序：

```powershell
cargo build --release
```

预期路径：`target/release/wacu.exe`。首次构建 Slint 的图形依赖会较慢，之后增量构建更快。

## 性能

- Rust 单进程后台核心；Slint 原生绘图，无 WebView2 子进程。
- Mini 原生窗口保持事件循环，无通知区图标；完整胶囊、详情和设置按需显示。
- 空闲状态无动画帧循环、无网络轮询；界面动效仅在用户交互时运行。
- 当前 Mini 模式在 Windows 测试机上测得：纯 Mini 常驻私有内存约 67.9 MB、工作集约 72 MB，6 秒静置 CPU 样本 0ms；打开设置期间约 98.8 MB 私有内存。这是初步测试数据，不代表所有老电脑。
- 后续仍需验证真实鼠标点击响应、旧显卡渲染及长期驻留的稳定性。

## 开发计划

第一阶段：原生 UI、Mini 悬浮入口、单击展开/收起、Win32 拖动与窗口定位、演示用量。

第二阶段：Codex 真实额度适配、Claude Code、WorkBuddy 官方授权可行性验证。

第三阶段：增量采集、缓存、配额提醒、安装包、自动更新。

详细内容见 [UI 设计落地规范与实际截图](docs/UI_SPEC.md)、[架构文档](docs/ARCHITECTURE.md) 和 [Windows 验收记录](docs/WINDOWS_QA.md)。

Wacu 是独立的桌面 Mini 悬浮窗口，不是 Windows 天气 Widgets 或任务栏内嵌控件，也不修改 Explorer。
