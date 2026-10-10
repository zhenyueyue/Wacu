export type ServiceId = "codex" | "claude" | "workbuddy";
export type DisplayMode = "popup" | "pinned";

export interface QuotaWindow {
  label: string;
  remainingPercent: number;
  resetLabel?: string;
  balance?: string;
}

export interface UsageService {
  id: ServiceId;
  name: string;
  subtitle: string;
  color: string;
  remainingPercent: number;
  summary: string;
  windows: QuotaWindow[];
}

export interface Preferences {
  visibleServices: ServiceId[];
  lowThreshold: number;
  criticalThreshold: number;
  showRemaining: boolean;
}

export const defaults: Preferences = {
  visibleServices: ["codex", "claude", "workbuddy"],
  lowThreshold: 20,
  criticalThreshold: 10,
  showRemaining: true,
};

const STORAGE_KEY = "wacu:preferences:v1";

export function loadPreferences(): Preferences {
  try {
    const value = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<Preferences> | null;
    if (!value || !Array.isArray(value.visibleServices)) return { ...defaults };
    return {
      ...defaults,
      ...value,
      visibleServices: defaults.visibleServices.filter((id) => value.visibleServices?.includes(id)),
    };
  } catch {
    return { ...defaults };
  }
}

export function savePreferences(preferences: Preferences) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences));
}

// Phase 1 mock fixtures: all values are demonstrations, never real account data.
export const demoServices: UsageService[] = [
  {
    id: "codex",
    name: "Codex",
    subtitle: "OpenAI · ChatGPT",
    color: "#8bd7be",
    remainingPercent: 76,
    summary: "5 小时额度",
    windows: [
      { label: "5 小时额度", remainingPercent: 76, resetLabel: "1 小时 42 分后" },
      { label: "周期额度", remainingPercent: 62, resetLabel: "4 天 6 小时后" },
    ],
  },
  {
    id: "claude",
    name: "Claude Code",
    subtitle: "Anthropic · Claude",
    color: "#e5ad91",
    remainingPercent: 43,
    summary: "当前会话额度",
    windows: [
      { label: "当前会话额度", remainingPercent: 43, resetLabel: "3 小时 18 分后" },
      { label: "周期额度", remainingPercent: 61, resetLabel: "2 天 12 小时后" },
    ],
  },
  {
    id: "workbuddy",
    name: "WorkBuddy",
    subtitle: "Credits · 积分",
    color: "#a49af2",
    remainingPercent: 92,
    summary: "3,324 积分剩余",
    windows: [
      { label: "积分余额", remainingPercent: 92, balance: "3,324 / 3,609" },
    ],
  },
];

export function quotaColor(remaining: number, preferences: Preferences) {
  if (remaining <= preferences.criticalThreshold) return "#f27f89";
  if (remaining <= preferences.lowThreshold) return "#f2bf78";
  return "#8bd7be";
}
