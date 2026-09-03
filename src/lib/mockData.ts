import type { UsageData } from "./types";

// Fallback dataset, used when no local Claude logs are found (or outside
// Tauri). Mirrors a heavy-usage afternoon so the UI looks alive on first run.
export function mockUsage(): UsageData {
  const now = Date.now();
  return {
    isMock: true,
    generatedAt: now,
    session: {
      startedAt: now - (3 * 3600 + 12 * 60) * 1000,
      fromLive: false,
      prompts: 139,
      tokens: 1_842_300,
    },
    today: { tokens: 1_842_300, cost: 7.94, prompts: 279 },
    month: {
      startedAt: new Date(new Date().getFullYear(), new Date().getMonth(), 1).getTime(),
      tokens: 38_400_000,
      cost: 168.5,
      prompts: 5_240,
    },
    models: [
      { id: "fable", name: "Fable", tokens: 812_400, cost: 9.62, prompts: 58 },
      { id: "opus", name: "Opus", tokens: 571_900, cost: 3.94, prompts: 71 },
      { id: "sonnet", name: "Sonnet", tokens: 274_000, cost: 0.81, prompts: 62 },
      { id: "haiku", name: "Haiku", tokens: 184_000, cost: 0.29, prompts: 46 },
    ],
    // tokens per 15-min bin from the session start (3h12m in → ~13 bins elapsed)
    burn: [61e3, 88e3, 74e3, 132e3, 190e3, 221e3, 175e3, 288e3, 342e3, 296e3, 231e3, 372e3, 160e3, 0, 0, 0, 0, 0, 0, 0],
  };
}
