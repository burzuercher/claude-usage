import { describe, it, expect } from "vitest";
import { allModelsBucket, sessionPace, weeklyPace } from "./pace";
import type { RealBucket, RealUsage, UsageData, Week } from "./types";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;
const NOW = Date.parse("2026-09-23T12:00:00Z");

function bucket(utilization: number, resetsIn: number, key = "seven_day"): RealBucket {
  return {
    key,
    label: "All models",
    utilization,
    resetsAt: new Date(NOW + resetsIn).toISOString(),
    severity: "",
    isActive: false,
    note: "",
  };
}

/** Local weekly totals aligned with a window that resets `resetsIn` from now. */
function week(resetsIn: number, cost: number, cost24h: number, fromLive = true): Week {
  return { startedAt: NOW + resetsIn - 7 * DAY, fromLive, cost, cost24h };
}

describe("weeklyPace", () => {
  it("projects the last 24h of usage to the reset", () => {
    // Half used, 3.5 days left; the last day was 20% of the window's usage → 10%/day.
    const p = weeklyPace(bucket(0.5, 3.5 * DAY), week(3.5 * DAY, 100, 20), NOW)!;
    expect(p.basis).toBe("day");
    expect(p.perDay).toBeCloseTo(0.1);
    expect(p.budgetPerDay).toBeCloseTo(0.5 / 3.5);
    expect(p.willBust).toBe(false);
    expect(p.projectedPct).toBe(85);
    expect(p.capAt).toBe(0);
  });

  it("reports when a day-long pace hits the cap before reset", () => {
    // 20%/day with 50% left → capped in 2.5 days, a day before the reset.
    const p = weeklyPace(bucket(0.5, 3.5 * DAY), week(3.5 * DAY, 100, 40), NOW)!;
    expect(p.willBust).toBe(true);
    expect(p.projectedPct).toBe(100);
    expect(p.capAt).toBeCloseTo(NOW + 2.5 * DAY, -3);
  });

  it("falls back to the window average when local logs describe another window", () => {
    const stale = week(3.5 * DAY - 2 * DAY, 100, 40);
    const p = weeklyPace(bucket(0.5, 3.5 * DAY), stale, NOW)!;
    expect(p.basis).toBe("average");
    expect(p.perDay).toBeCloseTo(0.5 / 3.5);
  });

  it("does not project a window less than a day old", () => {
    const p = weeklyPace(bucket(0.06, 7 * DAY - 10 * HOUR), week(7 * DAY - 10 * HOUR, 10, 10), NOW)!;
    expect(p.basis).toBe("early");
    expect(p.perDay).toBeCloseTo(0.06);
    expect(p.willBust).toBe(false);
  });

  it("is absent without a future reset", () => {
    expect(weeklyPace(undefined, week(DAY, 1, 1), NOW)).toBeUndefined();
    expect(weeklyPace(bucket(0.5, -HOUR), week(DAY, 1, 1), NOW)).toBeUndefined();
  });
});

describe("allModelsBucket", () => {
  it("picks the all-models window, not a per-model one", () => {
    const real = { found: true, weekly: [bucket(0.9, DAY, "model:fable"), bucket(0.4, DAY)] } as RealUsage;
    expect(allModelsBucket(real)?.utilization).toBe(0.4);
  });
});

describe("sessionPace", () => {
  it("projects the last 30 minutes of burn, calibrated to the live %", () => {
    // 2.5h into the window at 50%; 200k tokens so far, 80k of it in the last 30 min.
    const resetT = NOW + 2.5 * HOUR;
    const real = { found: true, session: { ...bucket(0.5, 2.5 * HOUR, "five_hour") } } as RealUsage;
    const burn = new Array(20).fill(0);
    burn[8] = 40_000;
    burn[9] = 40_000;
    burn[0] = 120_000;
    const data = {
      session: { startedAt: resetT - 5 * HOUR, fromLive: true, prompts: 10, tokens: 200_000 },
      burn,
    } as UsageData;
    const p = sessionPace(real, data, NOW);
    expect(p.forecast?.basis).toBe("recent");
    // 80k tokens / 30 min = 20% per 30 min → the remaining 50% in 75 min.
    expect(p.forecast?.willBust).toBe(true);
    expect(p.forecast?.label).toBe("1h 15m");
  });
});
