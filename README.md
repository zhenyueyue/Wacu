# Wacu

> A little watcher for your AI. 静静守望，每一份余量。

Windows 轻量 AI 用量守望工具。**现已完全重写为 Rust + Slint + Windows API**；运行时不使用 React、Tauri、Electron 或 WebView2。

## 界面

- **桌面 Mini 悬浮图标是唯一正式入口**：启动后显示约 66×66px 圆形图标，单击展开纵向用量胶囊；单击胶囊顶部头像再收回 Mini，不使用系统托盘图标。
- Mini 与完整胶囊支持区分单击、拖动：移动指针超过 7px 才开始系统原生拖动，松开后保存 Mini 位置。右键 Mini 打开设置，可在“关于 → 退出 Wacu”正常结束进程。
- EXE 与原生窗口统一使用品牌 Logo；程序内嵌 16–256px 多尺寸 Windows 图标。
- 胶囊顶部显示品牌 Logo，下方排列 Claude Code、Codex、WorkBuddy，以双眼角色、带背景轨道的进度环和已用百分比显示演示用量；设置按钮与服务区分开留白。默认标准档主体宽 72px、圆环 56px；外观页可选择紧凑／标准／宽松三档（窗口宽分别为 88／96／104px）并记住偏好，旧标准升级后自动对应新的紧凑档。
- 详情与设置采用紧凑排版：快览 296×422px、完整详情 296×612px、设置 460×600px；正文 13–14px、百分比 32px，使用石墨色渐变、圆角控件和柔和阴影。
- Mini 常驻于桌面，展开后完整胶囊取代 Mini；收回时按胶囊当前位置恢复 Mini。Mini 独立保存位置，保留原有三档胶囊大小与服务显示设置，详情面板打开后仍可正常操作。
- 进度环使用 Slint 原生路径与圆头端点；Mini、胶囊与详情沿用短时淡入淡出动画，服务仍可显示选中高亮，所有动效可统一关闭。
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
