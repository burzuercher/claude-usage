// Mirrors the Rust `UsageData` struct returned by the `get_usage` command.

export interface Window5h {
  startedAt: number; // ms epoch — current 5-hour block start
  prompts: number;
  tokens: number;
}

export interface Weekly {
  startedAt: number;
  prompts: number;
  tokens: number;
  opusPrompts: number;
  opusTokens: number;
}

export interface Today {
  tokens: number;
  cost: number;
  prompts: number;
}

export interface Month {
  startedAt: number; // ms epoch — first day of current calendar month
  tokens: number;
  cost: number;
  prompts: number;
}

export interface ModelStat {
  id: string; // "opus" | "sonnet" | "haiku" | "other"
  name: string;
  tokens: number;
  cost: number;
  prompts: number;
}

export interface SurfaceStat {
  id: string; // "code" | "chat" | "other"
  name: string;
  prompts: number;
  tokens: number;
}

export interface DailyStat {
  label: string;
  code: number;
  chat: number;
  other: number;
}

export interface RecentTask {
  task: string;
  tokens: number;
  model: string; // family id
  t: string; // relative time
  ts: number;
}

export interface HeatRow {
  label: string;
  cells: number[]; // 24 values, 0..1
}

export interface UsageData {
  isMock: boolean;
  generatedAt: number;
  session: Window5h;
  weekly: Weekly;
  today: Today;
  month: Month;
  models: ModelStat[];
  surfaces: SurfaceStat[];
  burn: number[];
  daily: DailyStat[];
  recent: RecentTask[];
  heatmap: HeatRow[];
}

// ─── Settings ──────────────────────────────────────────────────────────────

export type PlanTier = "pro" | "max5" | "max20" | "team";

export interface Account {
  found: boolean;
  email: string;
  orgName: string;
  orgType: string;
  seatTier: string;
  userRateLimitTier: string;
  detectedPlan: PlanTier | "";
}

export interface RealBucket {
  key: string;
  label: string;
  utilization: number; // 0..1
  resetsAt: string; // ISO timestamp, may be ""
}

export interface ExtraUsage {
  isEnabled: boolean;
  usedCredits: number; // $ spent on extra usage this billing month
  monthlyLimit: number; // $ cap (0 = none)
  currency: string;
  utilization: number; // 0..1 of the extra-usage budget
  disabledReason: string;
}

export interface RealUsage {
  found: boolean;
  reason: string;
  session: RealBucket | null;
  weekly: RealBucket[];
  extra: ExtraUsage | null;
  fetchedAt: number;
}

export interface Environment {
  id: string; // ".claude*" dir name or absolute custom path
  label: string;
  account: Account;
  hasLogs: boolean;
  custom?: boolean; // user-added path (not auto-discovered)
}

export interface Settings {
  env: string; // selected environment id ("" = default/.claude)
  customEnvs: string[]; // user-added environment paths
  hiddenEnvs: string[]; // discovered env ids hidden from the switcher
  plan: PlanTier; // manual fallback when autoPlan is off
  autoPlan: boolean; // follow the selected account's detected plan
  seats: number; // Team plan seat count (min enforced by plan def)
  showSpend: boolean; // show the Spend tab + Overview expenditure card
  dark: boolean;
  compact: boolean;
  accent: string;
}

// Per-model display colors (CSS vars resolved at render).
export const MODEL_COLOR: Record<string, string> = {
  sonnet: "var(--clay)",
  opus: "var(--ink)",
  haiku: "var(--sand)",
  other: "var(--moss)",
};
