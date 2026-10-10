# Wacu

> A little watcher for your AI.

Wacu 是面向 Windows 的轻量 AI 用量监控小组件。产品核心是可点击弹出的纵向胶囊，亦支持常驻悬浮显示。

## 当前进度

**0.1.0 界面原型**：包含 Windows 系统托盘入口、两种显示模式、圆形剩余进度、服务详情、设置与本地保存。**目前所有额度数字均为模拟数据，尚未接入 Codex / Claude Code / WorkBuddy 账号或 API。**

## 本地运行

需要 Node.js、Rust 与 Windows 下的 WebView2 / Microsoft C++ 构建环境。

```powershell
npm install
npm run tauri dev
```

单独预览前端界面：

```powershell
npm run dev
```

浏览器可通过 `?view=settings` 和 `?view=detail&service=claude` 预览其他窗口。

## 交互

- 托盘左键切换胶囊显示；右键菜单提供显示、设置、退出。
- 弹出模式：默认隐藏，点击托盘弹出并跟随入口定位，失去焦点自动隐藏；常驻模式：置顶，可用顶部图标拖动，位置自动保存。
- 点击圆形额度图标展示对应服务详情；胶囊底部可切换模式或打开设置。
- 设置页面可选择展示的服务、百分比显示、演示提醒阈值。

**注意：** 当前的任务栏入口是系统托盘图标，不是 Windows 11 左下角天气所在的原生 Widgets 按钮。后续可探索独立的任务栏快速入口，避免不稳定地注入 Explorer。

## 开发文档

- [产品规范](docs/PRD.md)
- [技术架构](docs/ARCHITECTURE.md)
- [Windows 实机验收记录](docs/WINDOWS_QA.md)
