import type { UsageData } from "./types";

// Fallback dataset (the prototype's STATE), used when no local Claude logs are
// found. Mirrors a heavy-usage afternoon so the UI looks alive on first run.
export function mockUsage(): UsageData {
  const now = Date.now();

  // 7×24 workday-ish heatmap.
  const dayInit = ["M", "T", "W", "T", "F", "S", "S"];
  const heatmap = Array.from({ length: 7 }, (_, day) => ({
    label: dayInit[day],
    cells: Array.from({ length: 24 }, (_, hr) => {
      const base = hr >= 9 && hr <= 18 ? 0.6 : 0.1;
      const dayFactor = day === 5 || day === 6 ? 0.3 : 1;
      const noise = ((hr * 7 + day * 13) % 10) / 25; // deterministic
      return Math.min(1, (base + noise) * dayFactor);
    }),
  }));

  return {
    isMock: true,
    generatedAt: now,
    session: {
      startedAt: now - (3 * 3600 + 12 * 60) * 1000,
      prompts: 139, // ≈62% of Max 5× session budget
      tokens: 1_842_300,
    },
    weekly: {
      startedAt: now - (4 * 24 * 3600 + 8 * 3600) * 1000,
      prompts: 1296,
      tokens: 12_400_000,
      opusPrompts: 374,
      opusTokens: 4_100_000,
    },
    today: { tokens: 1_842_300, cost: 7.94, prompts: 279 },
    month: {
      startedAt: new Date(new Date().getFullYear(), new Date().getMonth(), 1).getTime(),
      tokens: 38_400_000,
      cost: 168.5,
      prompts: 5_240,
    },
    models: [
      { id: "sonnet", name: "Sonnet", tokens: 1_086_400, cost: 3.21, prompts: 162 },
      { id: "opus", name: "Opus", tokens: 571_900, cost: 3.94, prompts: 71 },
      { id: "haiku", name: "Haiku", tokens: 184_000, cost: 0.79, prompts: 46 },
    ],
    surfaces: [
      { id: "code", name: "Claude Code", prompts: 142, tokens: 980_000 },
      { id: "chat", name: "Claude Desktop", prompts: 103, tokens: 620_000 },
      { id: "other", name: "Other", prompts: 34, tokens: 242_300 },
    ],
    burn: [3, 5, 4, 8, 12, 14, 11, 18, 22, 19, 15, 24, 21, 17, 23, 26, 19, 14, 11, 9],
    daily: [
      { label: "Mon", code: 14, chat: 22, other: 4 },
      { label: "Tue", code: 38, chat: 18, other: 8 },
      { label: "Wed", code: 52, chat: 14, other: 6 },
      { label: "Thu", code: 28, chat: 24, other: 11 },
      { label: "Fri", code: 41, chat: 16, other: 9 },
      { label: "Sat", code: 9, chat: 6, other: 2 },
      { label: "Sun", code: 31, chat: 20, other: 7 },
    ],
    recent: [
      { task: "refactor auth flow", tokens: 184_200, model: "sonnet", t: "14m ago", ts: now - 14 * 60000 },
      { task: "write release notes", tokens: 42_800, model: "haiku", t: "38m ago", ts: now - 38 * 60000 },
      { task: "debug websocket timeout", tokens: 312_400, model: "opus", t: "1h ago", ts: now - 60 * 60000 },
      { task: "review pricing page", tokens: 91_600, model: "sonnet", t: "1h 24m", ts: now - 84 * 60000 },
    ],
    heatmap,
  };
}
