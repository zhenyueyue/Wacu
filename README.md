# Wacu

> A little watcher for your AI. 静静守望，每一份余量。

Windows 轻量 AI 用量守望工具。**现已完全重写为 Rust + Slint + Windows API**；运行时不使用 React、Tauri、Electron 或 WebView2。

## 界面

- Windows 系统托盘左键打开/收起黑色纵向胶囊；右键可进入设置、切换模式或退出。
- 胶囊按设计图排列 Claude Code、Codex、WorkBuddy，以彩色圆环、双眼角色和已用百分比显示演示用量；默认主体宽 64px、圆环 50px，外观页可选择紧凑／标准／宽松三档并记住偏好。
- 详情与设置在原设计网格上整体放大 25%，正文约 15–16px，百分比 40px，按钮与图标同步放大。
- 可在“点击弹出”与“常驻悬浮”之间切换；拖动位置和服务显示开关将保存在本地。胶囊高度随启用服务数量自动调整，关闭服务后其他项目仍能正确打开详情。
- 进度环使用 Slint 原生路径绘制；提供悬停、选中蓝色光晕与指示箭头、按下缩小反馈，所有过渡为 100–200ms，可统一关闭。
- 点击服务先弹出窄版用量快览，再展开带示意趋势图的完整详情。
- 深色分栏设置按设计图提供开机启动、常驻、贴边隐藏、动画、透明度和左右位置；用户偏好本地保存。开机启动操作当前用户的 Windows Run 注册表项，默认不启用；贴边隐藏保留边缘触点，鼠标移入可恢复。
- 详细面板提供今日／7天／30天／全部筛选、24 根示意柱和官网入口。开源／反馈／帮助／最新版本按钮使用系统默认浏览器；当前语言仅提供简体中文，更新入口查看 GitHub Releases，不自动安装。
- **重要：目前全部百分比均为演示数据，尚未接入实际账户或 API。**

## 开发与运行

支持 Windows x64，安装 Rust MSVC 工具链和 Visual Studio C++ Build Tools。

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
- 系统托盘保持事件循环；窗口默认隐藏，按需显示。
- 空闲状态无动画帧循环、无网络轮询；界面动效仅在用户交互时运行。
- 已在当前 Windows 测试机上测得 Release 隐藏模式约 60 MB 私有内存、常驻模式约 71 MB，10 秒静置 CPU 样本约 0%；这是初步数据，不代表所有老电脑。
- 后续仍需验证真实鼠标点击响应、旧显卡渲染及长期驻留的稳定性。

## 开发计划

第一阶段：原生 UI 重写、系统托盘、两种模式、Win32 窗口定位、模拟用量。

第二阶段：Codex 真实额度适配、Claude Code、WorkBuddy 官方授权可行性验证。

第三阶段：增量采集、缓存、配额提醒、安装包、自动更新。

详细内容见 [UI 设计落地规范与实际截图](docs/UI_SPEC.md)、[架构文档](docs/ARCHITECTURE.md) 和 [Windows 验收记录](docs/WINDOWS_QA.md)。

Wacu 的入口是 Windows 系统托盘，尚非系统天气 Widgets 原生任务栏区域。
