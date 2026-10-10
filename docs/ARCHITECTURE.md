# Wacu · 技术架构 v0.1

## 栈

- 桌面：Tauri 2 / Rust
- UI：React 19 / TypeScript / CSS（后续可引入 shadcn/ui）
- 发布目标：Windows x64，NSIS 安装包
- 第一阶段不需要后台服务、数据库或平台密钥

## 模块

```
System tray ──┐
             ├─ Rust Window Manager ── Capsule / Detail / Settings
React UI ────┘           │
                        Mode state (app_config_dir/display-mode.json)

Future Provider Manager
├─ CodexAdapter        (本机 CLI/app-server)
├─ ClaudeCodeAdapter   (官方/授权接口优先 + 本地统计)
├─ WorkBuddyAdapter    (先验证官方开放平台权限)
└─ API Providers       (独立账单模式)
        │
        └─ UsageSnapshot -> Cache -> UI
```

## 当前窗口

| 标签 | 职责 | 特点 |
| --- | --- | --- |
| `capsule` | 快速浏览剩余百分比 | 114x458，透明、无边框、置顶，不占任务栏 |
| `detail` | 单个平台多额度详情 | 336x474，透明、无边框 |
| `settings` | 显示模式、服务及偏好 | 506x660，普通原生窗口 |

Rust commands: `get_display_mode`、`set_display_mode`、`toggle_capsule`、`show_settings`、`show_detail`、`get_selected_service`。
跨窗口同步通过 `wacu:mode-changed`、`wacu:detail-service` 与 `wacu:preferences-changed`。

## 数据契约（下一阶段）

```ts
interface UsageSnapshot {
  serviceId: string;
  source: "subscription" | "api";
  status: "ok" | "stale" | "unauthorized" | "unsupported" | "error";
  collectedAt: string;
  windows: Array<{
    id: string;
    label: string;
    remainingPercent?: number;
    used?: number;
    total?: number;
    unit?: "tokens" | "credits" | "requests" | "currency";
    resetsAt?: string;
  }>;
}
```

缺失字段不得用 0 或 100 替代。对每个采集器设置缓存/超时/退避，并明确定义数据状态和权限边界。

## 后续必须处理的 Windows 特性

1. 任务栏位置/自动隐藏与每屏 DPI 缩放需要真机验证；定位已使用屏幕工作区。
2. Popup 已改为托盘点击锚点定位，不再固定主屏幕右下角。
3. Pinned 已保存/恢复物理像素坐标，离屏恢复会约束回屏幕工作区；自动吸边待做。
4. 动态胶囊高度和多项内容滚动另行开发，需要专门测试原生窗口调整尺寸。
4. Explorer 重启时的托盘恢复、应用单实例、启动和退出策略。
5. 根据内容项数动态调整胶囊高度；完整键盘与无障碍交互。
