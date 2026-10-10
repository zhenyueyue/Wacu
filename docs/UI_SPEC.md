# Wacu · 两页设计图落地规范

日期：2026-10-10。对应用户提供的桌面用量监控组件设计图：胶囊、详情、设置，以及组件、状态、尺寸和颜色规范。

## 本轮实现

| 设计要求 | 程序实现 |
| --- | --- |
| 黑色窄胶囊、圆形角色、百分比、底部齿轮 | 默认 64px 主体、50px 进度环、28px 双眼角色、14px 数字、40px 设置触点；三档大小可选 |
| 点击后在侧面展开，不覆盖胶囊 | 优先右侧，空间不足时左侧；保持屏幕工作区内；蓝色箭头指向面板 |
| 快览：大百分比、进度条、三行信息、详情按钮 | 已用百分比与次数、今日用量、剩余额度、重置时间 |
| 完整详情：时间筛选、柱状图、官网入口 | 今日／7天／30天／全部，24 根演示柱、网格和五个刻度；打开默认浏览器 |
| 设置：左导航、右内容、底部链接 | 常规／服务管理／外观／行为／关于，圆角深色窗口与 Fluent 控件 |
| 常规设置 | 开机启动、常驻显示、贴边隐藏、显示动画、透明度、左右位置、简体中文、最新版本入口 |
| 悬停、选中、按下 | 背景微亮、蓝色环绕光晕和箭头、圆环缩小及降低亮度 |
| 动画与透明度 | 100–200ms ease-out；动画开关同时控制胶囊、详情、图表、设置；透明度保留 65%–100% |

当前只有 Claude Code、Codex、WorkBuddy 三项服务，按原图橙／绿／紫顺序排列；开关仍使用稳定服务 ID，不因排序和过滤错配。没有为了凑满六项添加不存在的服务。

## 尺寸和配色

以下均为逻辑像素。胶囊在主体外留出 12px，详情和设置留出 15px，容纳透明阴影；这部分不计入设计主体尺寸。详情、设置及共用控件由统一 1.25 倍 Token 放大，字体和坐标同步变化。

| 组件 | 尺寸 |
| --- | --- |
| 胶囊主体 | 紧凑／标准／宽松宽度 48／64／80px；默认三项主体 64 × 282px |
| 胶囊原生窗口 | 默认 88 × 306px；0／1／2／3 项对应 166／166／236／306px；紧凑三项 72 × 244px，宽松三项 104 × 356px |
| 服务项 | 标准 56 × 70px；圆环 50px；中心角色 28px；数字 14px |
| 底部设置触点 | 紧凑／标准／宽松为 32／40／48px |
| 快览原生窗口 | 370 × 500px；深色主体 340 × 470px |
| 完整详情原生窗口 | 370 × 730px；深色主体 340 × 700px |
| 设置原生窗口 | 530 × 700px；主体 500 × 670px；左导航 145px |
| 信息行 | 40px 高；图标、标签、数值分别固定在 x=0／32.5／112.5 |
| 开关 | 45 × 25px，白色滑块 20px；设置项高 72.5px |
| 圆角 | 胶囊随宽度变化、面板 15px、控件 10px |

外观页的胶囊尺寸选择立即生效，并保存至用户配置。旧配置缺少 capsule_size 时采用标准档；调整后等待原生窗口重新布局，再约束到屏幕工作区并同步重定位已打开的详情。

| Token | 色值 |
| --- | --- |
| 主背景 | #0F0F10 |
| 面板 | #1A1A1D |
| 控件／悬停 | #26262B |
| 分割线 | #2F2F35 |
| 次要文字 | #A1A1AA |
| 辅助文字 | #71717A |
| 选中与强调 | #3B82F6／#0085FF |
| Codex | #22C55E |
| Claude Code | #FF8A4C |
| WorkBuddy | #8B5CF6 |

图标均为随程序内嵌的 SVG；圆环与角色由 Slint 原生绘制。没有引入图片生成、网页或 WebView 运行时。

## 功能边界

- 所有额度与图表仍是演示数据，详情标题旁和服务管理／关于页明确标识。Claude 示例为 240 / 1,000 次，已用 24%，剩余 760 次；三个服务均校验已用 + 剩余 = 总额。
- 开机启动仅在用户操作开关时修改当前用户 Run 注册表项，默认关闭；状态读取匹配当前程序路径的注册表项。Windows 实际重新登录启动尚未验收。
- 常驻模式靠近边缘可收起，保留 6px 逻辑触点；移入后恢复并保留 800ms 进入时间。配置只保存正常位置，隐藏坐标不会写入配置；算法包含负坐标副屏及 DPI 换算检查。
- 详情保持焦点或鼠标位于详情内时，弹出胶囊不提前收起。关闭当前选中服务时同步关闭详情并清除选中状态。
- 语言目前仅支持简体中文。版本按钮查看 GitHub Releases，不显示虚假的“已是最新版本”，不自动下载或安装。
- 设置页错误提示用于注册表、偏好保存及浏览器打开失败。真实登录账户、自动更新和长时间运行仍属后续范围。

## 实际原生截图

下列图片来自 Windows Slint Debug 进程的帧快照：

[界面总览](screenshots/overview.png) 将未改动的原生快照排列在一起，便于与设计图比较。

- [标准胶囊](screenshots/capsule.png)、[紧凑胶囊](screenshots/capsule-compact.png)、[宽松胶囊](screenshots/capsule-spacious.png)、[选中状态](screenshots/capsule-selected.png)、[全服务关闭](screenshots/capsule-empty.png)
- [用量快览](screenshots/quick-detail.png)、[完整详情](screenshots/full-detail.png)
- [常规设置](screenshots/settings-general.png)、[服务管理](screenshots/settings-services.png)、[外观](screenshots/settings-appearance.png)、[行为](screenshots/settings-behavior.png)、[关于](screenshots/settings-about.png)
- [7 天](screenshots/detail-week.png)、[30 天](screenshots/detail-month.png)、[全部](screenshots/detail-all.png)

检查中特别修复了 Slint 自由布局子元素默认居中导致的标题栏漂移、百分比居中、进度填充居中、图标挤占数值列和滑杆标签重叠。标题栏和各列使用明确的 x/y/width/height；所有分页已逐张检查。

## 可重复截图

```powershell
cargo build
.\scripts\capture-ui.ps1             # 在 target 中生成 17 张快照
.\scripts\capture-ui.ps1 -Archive    # 将同一批快照更新到 docs/screenshots
python scripts\ui-overview.py        # 生成总览，需要 Pillow
```

每个预览进程采用默认测试配置，生成截图后退出，不保存胶囊位置、不修改用户设置或启动项。环境变量只设置在子进程上。Release 不提供这些调试入口。

也可手动运行：

```powershell
$env:WACU_QA_VIEW = "detail"   # capsule / quick / detail / settings
$env:WACU_QA_PAGE = "0"        # 设置分类 0–4
$env:WACU_QA_COUNT = "3"       # 胶囊服务数 0–3
$env:WACU_QA_SIZE = "1"        # 胶囊大小 0–2
$env:WACU_QA_PERIOD = "0"      # 详情时间筛选 0–3
cargo run
```

原生快照证明绘制和排版；完整真实鼠标、Windows 重新登录、跨屏拖动及旧显卡验收仍须另行进行。
