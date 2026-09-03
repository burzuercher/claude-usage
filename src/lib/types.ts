// Mirrors the Rust `UsageData` struct returned by the `get_usage` command.

export interface Window5h {
  startedAt: number; // ms epoch — current session window start
  fromLive: boolean; // true when the start came from the live usage API (matches the ring)
  prompts: number;
  tokens: number;
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
  id: string; // "fable" | "opus" | "sonnet" | "haiku" | "other"
  name: string;
  tokens: number;
  cost: number;
  prompts: number;
}

export interface UsageData {
  isMock: boolean;
  generatedAt: number;
  session: Window5h;
  today: Today;
  month: Month;
  /** Per-model split for the current session window. */
  models: ModelStat[];
  /** Tokens per 15-min bin across the session window (20 bins = 5h), from the window start. */
  burn: number[];
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

// ─── Live usage (mirrors Rust `realusage::RealUsage`) ──────────────────────

export interface RealBucket {
  key: string;
  label: string;
  utilization: number; // 0..1
  resetsAt: string; // ISO timestamp, may be ""
  severity: string; // server hint, may be ""
  isActive: boolean; // server flag: this window is the binding one
  note: string; // promo / boost notice attached to this bar, may be ""
}

export interface ExtraUsage {
  isEnabled: boolean;
  usedDollars: number; // extra-usage spend this billing month (currency units)
  limitDollars: number; // cap (0 = none)
  currency: string;
  utilization: number; // 0..1 of the extra-usage budget
  disabledReason: string;
  spendLimitReached: boolean;
  canPurchaseCredits: boolean;
}

export type RealSource = "api" | "cache" | "";

export interface RealUsage {
  found: boolean;
  reason: string;
  /** "api" = fetched by us; "cache" = Claude Code's own cached response. */
  source: RealSource;
  session: RealBucket | null;
  weekly: RealBucket[];
  extra: ExtraUsage | null;
  fetchedAt: number; // ms epoch the data was fetched from Anthropic
  httpStatus: number; // last API attempt status (0 = none / transport error)
  retryAfterSecs: number; // server-suggested retry delay on 429 (0 = none)
  promoCachedAt: number; // ms epoch the promo-notice flags were cached (0 = none)
}

// ─── Local attribution (mirrors Rust `attribution::Attribution`) ───────────
// The other half of Claude Code's /usage screen: what is driving usage, derived
// from local session transcripts rather than the live usage API. Machine-local
// and approximate, exactly as the CLI's own screen is.

export interface Behavior {
  /** "cache_miss" | "long_context" | "subagent_heavy" | "high_parallel" | "cron" */
  key: string;
  pct: number; // 0..100 share of the window's usage
  /** Requests for cache_miss/long_context/high_parallel; sessions for the rest. */
  count: number;
}

export interface Share {
  name: string;
  pct: number; // 0..100 (entries rounding to 0 are dropped by the backend)
}

export interface AttrWindow {
  requestCount: number;
  sessionCount: number;
  /** Overlapping characteristics of the usage, not a partition — these do not sum to 100. */
  behaviors: Behavior[];
  /** Subagents, keyed by the skill running inside them when there is one. */
  agents: Share[];
  skills: Share[];
  plugins: Share[];
  mcpServers: Share[];
}

export interface SessionStat {
  id: string;
  label: string; // generated title, else prompt slug, else a short id
  project: string;
  branch: string;
  startedAt: number; // ms epoch
  lastAt: number; // ms epoch
  turns: number;

  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  models: string[]; // raw ids, e.g. "claude-opus-5[1m]"

  /** True once Claude Code has written its cost record. A live session has none,
   *  so cost and the durations below stay 0 — the UI shows "—", never an estimate. */
  hasCostState: boolean;
  cost: number;
  apiMs: number;
  wallMs: number;
  toolMs: number;
  linesAdded: number;
  linesRemoved: number;
}

export interface Attribution {
  found: boolean;
  generatedAt: number;
  /** Transcripts read (mtime within the 7-day window). */
  scannedFiles: number;
  day: AttrWindow; // last 24h
  week: AttrWindow; // last 7d
  sessions: SessionStat[]; // newest first
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
  showSpend: boolean; // show the Overview expenditure card
  dark: boolean;
  compact: boolean;
  accent: string;
}

// Per-model display colors (CSS vars resolved at render).
export const MODEL_COLOR: Record<string, string> = {
  fable: "var(--plum)",
  opus: "var(--ink)",
  sonnet: "var(--clay)",
  haiku: "var(--sand)",
  other: "var(--moss)",
};
