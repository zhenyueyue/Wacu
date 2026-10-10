import { useCallback, useEffect, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Activity, ArrowLeft, Bell, Check, Eye,
  Info, LayoutDashboard, MousePointerClick, Pin, PinOff, RefreshCw, RotateCcw,
  Settings2, ShieldCheck, X,
} from "lucide-react";
import {
  demoServices, loadPreferences, quotaColor, savePreferences,
  type DisplayMode, type Preferences, type ServiceId, type UsageService,
} from "./data";
import "./App.css";

const native = isTauri();

function viewName() {
  if (native) return getCurrentWindow().label;
  return new URLSearchParams(window.location.search).get("view") ?? "capsule";
}

function usePreferences() {
  const [preferences, setPreferences] = useState<Preferences>(loadPreferences);
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("wacu:preferences-changed", () => {
      if (!disposed) setPreferences(loadPreferences());
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  const update = useCallback((next: Preferences) => {
    savePreferences(next);
    setPreferences(next);
    if (native) void emit("wacu:preferences-changed");
  }, []);
  return [preferences, update] as const;
}

function useMode() {
  const [mode, setMode] = useState<DisplayMode>("popup");
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void invoke<DisplayMode>("get_display_mode").then((value) => {
      if (!disposed) setMode(value);
    }).catch(console.error);
    void listen<DisplayMode>("wacu:mode-changed", (event) => {
      if (!disposed) setMode(event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  const update = async (next: DisplayMode) => {
    if (native) await invoke("set_display_mode", { mode: next });
    setMode(next);
  };
  return [mode, update] as const;
}

function ServiceMark({ id }: { id: ServiceId }) {
  if (id === "codex") {
    return <span className="service-mark service-mark-codex" aria-hidden="true">◎</span>;
  }
  if (id === "claude") {
    return <span className="service-mark service-mark-claude" aria-hidden="true">✳</span>;
  }
  return <span className="service-mark service-mark-workbuddy" aria-hidden="true">w</span>;
}

function QuotaRing({
  service, preferences, size = 57,
}: {
  service: UsageService; preferences: Preferences; size?: number;
}) {
  const radius = 23.5;
  const circumference = 2 * Math.PI * radius;
  const remaining = Math.max(0, Math.min(service.remainingPercent, 100));
  return (
    <span className="ring-wrap" style={{ width: size, height: size }}>
      <svg className="progress-ring" viewBox="0 0 56 56" width={size} height={size} role="img"
        aria-label={`${service.name} 演示剩余额度 ${remaining}%`}>
        <circle cx="28" cy="28" r={radius} className="ring-track" strokeWidth="4.5" />
        <circle cx="28" cy="28" r={radius} fill="none" stroke={quotaColor(remaining, preferences)}
          strokeWidth="4.5" strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={circumference * (1 - remaining / 100)}
          transform="rotate(-90 28 28)" className="ring-value" />
      </svg>
      <span className="ring-symbol"><ServiceMark id={service.id} /></span>
    </span>
  );
}

function Capsule() {
  const [preferences] = usePreferences();
  const [mode, setMode] = useMode();
  const [tip, setTip] = useState("演示数据 · 尚未连接账户");
  const shown = demoServices.filter((item) => preferences.visibleServices.includes(item.id));

  const openSettings = async () => {
    if (native) await invoke("show_settings");
    else window.location.search = "?view=settings";
  };
  const showDetail = async (id: ServiceId) => {
    if (native) await invoke("show_detail", { serviceId: id });
    else window.location.search = `?view=detail&service=${id}`;
  };
  const drag = () => {
    if (native && mode === "pinned") void getCurrentWindow().startDragging();
  };

  return (
    <div className="capsule-stage">
      <div className="capsule-shell">
        <div className={mode === "pinned" ? "capsule-grip is-grabbable" : "capsule-grip"}
          onMouseDown={drag} title={mode === "pinned" ? "拖动 Wacu" : "Wacu"}>
          <span className="tiny-light" />
          <Eye size={19} strokeWidth={1.8} />
          <span className="capsule-test-mark">演示</span>
        </div>
        <div className="capsule-divider" />
        <div className="capsule-services">
          {shown.length ? shown.map((service) => (
            <button type="button" key={service.id} className="capsule-service"
              title={`${service.name} · 演示剩余额度 ${service.remainingPercent}% · 点击查看详情`}
              onClick={() => void showDetail(service.id)}
              onMouseEnter={() => setTip(`${service.name} · ${service.summary} · 演示数据`)}
              onMouseLeave={() => setTip("演示数据 · 尚未连接账户")}>
              <QuotaRing service={service} preferences={preferences} />
              <span className="capsule-percent">{preferences.showRemaining ? `${service.remainingPercent}%` : service.name === "WorkBuddy" ? "WB" : service.id === "claude" ? "Claude" : "Codex"}</span>
            </button>
          )) : (
            <button className="empty-services" onClick={() => void openSettings()} type="button">+<span>添加</span></button>
          )}
        </div>
        <div className="capsule-divider" />
        <div className="capsule-actions">
          <button title={mode === "popup" ? "固定胶囊" : "切换为点击弹出"} aria-label="切换显示模式"
            type="button" onClick={() => void setMode(mode === "popup" ? "pinned" : "popup")}>
            {mode === "popup" ? <Pin size={15} /> : <PinOff size={15} />}
          </button>
          <button title="打开设置" aria-label="打开设置" type="button" onClick={() => void openSettings()}>
            <Settings2 size={16} />
          </button>
        </div>
      </div>
      <span className="sr-only" aria-live="polite">{tip}</span>
    </div>
  );
}

function Detail() {
  const [selected, setSelected] = useState<ServiceId>("codex");
  const [preferences] = usePreferences();
  const [note, setNote] = useState("");
  const close = async () => {
    if (native) await getCurrentWindow().hide();
    else window.location.search = "";
  };
  useEffect(() => {
    const requested = new URLSearchParams(window.location.search).get("service");
    if (requested && demoServices.some((service) => service.id === requested)) {
      setSelected(requested as ServiceId);
    }
    if (!native) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void invoke<ServiceId>("get_selected_service").then((id) => {
      if (!disposed) setSelected(id);
    }).catch(console.error);
    void listen<ServiceId>("wacu:detail-service", (event) => {
      if (!disposed) setSelected(event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => { disposed = true; unlisten?.(); };
  }, []);
  const service = demoServices.find((item) => item.id === selected) ?? demoServices[0];
  return (
    <div className="detail-stage">
      <section className="detail-panel">
        <div className="detail-header">
          <button className="icon-button" title="返回" onClick={() => void close()}><ArrowLeft size={18} /></button>
          <span className="detail-header-name">用量详情</span>
          <button className="icon-button" title="关闭" onClick={() => void close()}><X size={17} /></button>
        </div>
        <div className="detail-service-head">
          <div className="detail-service-icon" style={{ background: `${service.color}18`, color: service.color }}>
            <ServiceMark id={service.id} />
          </div>
          <div><h2>{service.name}</h2><p>{service.subtitle}</p></div>
        </div>
        <div className="detail-balance">
          <p>当前最需关注的额度</p>
          <div className="big-value">{service.remainingPercent}<span>%</span></div>
          <span>剩余额度 <span className="mock-badge">演示数据</span></span>
        </div>
        <div className="detail-quotas">
          {service.windows.map((quota) => (
            <div className="quota-entry" key={quota.label}>
              <div className="quota-label"><span>{quota.label}</span><strong>{quota.remainingPercent}%</strong></div>
              <div className="quota-track"><div style={{
                width: `${quota.remainingPercent}%`,
                background: quotaColor(quota.remainingPercent, preferences),
              }} /></div>
              <div className="quota-hint">{quota.balance ?? (quota.resetLabel ? `预计重置：${quota.resetLabel}` : "重置时间未知")}</div>
            </div>
          ))}
        </div>
        <div className="detail-bottom">
          <button className="light-action" onClick={() => setNote("当前仅展示模拟数据。下一阶段接入真实用量数据。")}>
            <RefreshCw size={15} /> 刷新用量
          </button>
          <span className="data-disclaimer"><Info size={13} /> 未连接账号</span>
        </div>
        {note && <p className="inline-note">{note}</p>}
      </section>
    </div>
  );
}

function Settings() {
  const [mode, setMode] = useMode();
  const [preferences, setPreferences] = usePreferences();
  const [message, setMessage] = useState("");
  const patch = (values: Partial<Preferences>) => {
    setPreferences({ ...preferences, ...values });
  };
  const resetPosition = async () => {
    try {
      if (native) await invoke("reset_capsule_position");
      setMessage(native ? "悬浮位置已恢复到当前主屏幕默认位置" : "位置重置仅在 Windows 应用中生效");
    } catch (error) {
      setMessage(`位置重置失败：${String(error)}`);
    }
  };
  return (
    <main className="settings-page">
      <div className="settings-hero">
        <div className="settings-brand"><div className="settings-brand-icon"><Eye size={21} /></div>
          <div><h1>Wacu</h1><p>A little watcher for your AI.</p></div>
        </div>
        <span className="version-pill">PREVIEW · 0.1.0</span>
      </div>
      <div className="settings-scroll">
        <div className="settings-section-title"><LayoutDashboard size={15} /> 窗口模式</div>
        <div className="mode-choices">
          <button className={`mode-card ${mode === "popup" ? "active" : ""}`}
            onClick={async () => { try { await setMode("popup"); setMessage("已切换为点击弹出模式"); } catch { setMessage("模式保存失败"); } }}>
            <div className="mode-icon"><MousePointerClick size={21} /></div>
            <span>点击弹出</span><small>点击托盘打开，失焦收起</small>
            {mode === "popup" && <Check size={16} className="mode-check" />}
          </button>
          <button className={`mode-card ${mode === "pinned" ? "active" : ""}`}
            onClick={async () => { try { await setMode("pinned"); setMessage("胶囊现在会常驻桌面"); } catch { setMessage("模式保存失败"); } }}>
            <div className="mode-icon"><Pin size={20} /></div>
            <span>常驻悬浮</span><small>置顶显示，可拖动位置</small>
            {mode === "pinned" && <Check size={16} className="mode-check" />}
          </button>
        </div>

        <button className="settings-reset-position" type="button" onClick={() => void resetPosition()}>
          <RotateCcw size={15} />
          <span>重置胶囊位置</span>
          <small>拖动后找不到胶囊时可以恢复位置</small>
        </button>

        <div className="settings-section-title spaced"><Activity size={15} /> 监控服务 <span>演示阶段</span></div>
        <div className="settings-card service-settings">
          {demoServices.map((service) => {
            const checked = preferences.visibleServices.includes(service.id);
            return (
              <label className="settings-provider" key={service.id}>
                <div className="provider-icon" style={{ color: service.color, backgroundColor: `${service.color}17` }}>
                  <ServiceMark id={service.id} />
                </div>
                <div className="provider-caption"><strong>{service.name}</strong><small>{service.subtitle}</small></div>
                <input type="checkbox" className="switch-input" checked={checked}
                  onChange={() => patch({ visibleServices: checked
                    ? preferences.visibleServices.filter((id) => id !== service.id)
                    : [...preferences.visibleServices, service.id] })} />
                <span className="switch-face" aria-hidden="true" />
              </label>
            );
          })}
        </div>

        <div className="settings-section-title spaced"><Bell size={15} /> 提醒阈值 <span>暂未启用通知</span></div>
        <div className="settings-card thresholds">
          <div className="threshold-row"><span>低额度提醒</span><strong>{preferences.lowThreshold}%</strong></div>
          <input aria-label="低额度提醒阈值" type="range" min="15" max="40" step="5"
            value={preferences.lowThreshold} onChange={(event) => patch({
              lowThreshold: Number(event.target.value),
              criticalThreshold: Math.min(preferences.criticalThreshold, Number(event.target.value) - 5),
            })} />
          <div className="threshold-row"><span>紧急提醒</span><strong>{preferences.criticalThreshold}%</strong></div>
          <input aria-label="紧急提醒阈值" type="range" min="5" max={Math.max(5, preferences.lowThreshold - 5)} step="5"
            value={preferences.criticalThreshold} onChange={(event) => patch({ criticalThreshold: Number(event.target.value) })} />
        </div>

        <div className="settings-section-title spaced"><Eye size={15} /> 展示偏好</div>
        <label className="settings-card toggle-preference">
          <div><strong>显示剩余百分比</strong><p>直接在胶囊圆环下方展示数字</p></div>
          <input type="checkbox" className="switch-input" checked={preferences.showRemaining}
            onChange={() => patch({ showRemaining: !preferences.showRemaining })} />
          <span className="switch-face" aria-hidden="true" />
        </label>
        <div className="settings-hint"><ShieldCheck size={16} /> 当前仅使用本地模拟数据，不读取账号令牌或对话内容。</div>
      </div>
      <footer className="settings-footer">
        <div className="settings-status"><span className="status-dot" /> 第一阶段 · 界面预览</div>
        <span>{message || "保持轻巧，安静守望。"}</span>
      </footer>
    </main>
  );
}

export default function App() {
  const view = viewName();
  if (view === "settings") return <Settings />;
  if (view === "detail") return <Detail />;
  return <Capsule />;
}
