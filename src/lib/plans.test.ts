import { describe, it, expect } from "vitest";
import { PLANS, isSeatPriced, computeEconomics } from "./plans";
import type { Month } from "./types";

const NOW = Date.UTC(2026, 4, 21); // May 21 2026
const monthStart = Date.UTC(2026, 4, 1); // May 1 2026

const month = (cost: number): Month => ({
  startedAt: monthStart,
  tokens: 0,
  cost,
  prompts: 0,
});

describe("isSeatPriced", () => {
  it("is true only for seat-priced plans", () => {
    expect(isSeatPriced(PLANS.team)).toBe(true);
    expect(isSeatPriced(PLANS.pro)).toBe(false);
    expect(isSeatPriced(PLANS.max20)).toBe(false);
  });
});

describe("computeEconomics — flat plans", () => {
  it("returns the subscription base and zero extra when no live data", () => {
    const e = computeEconomics(PLANS.pro, 1, month(50), NOW);
    expect(e.base).toBe(20);
    expect(e.extraUsage).toBe(0);
    expect(e.total).toBe(20);
    expect(e.projectedExtra).toBe(0);
    expect(e.projectedTotal).toBe(20);
    expect(e.apiValue).toBe(50); // local API-rate value, informational
  });

  it("uses the real extra-usage value when supplied", () => {
    const e = computeEconomics(PLANS.max5, 1, month(100), NOW, 42.5);
    expect(e.base).toBe(100);
    expect(e.extraUsage).toBe(42.5);
    expect(e.total).toBeCloseTo(142.5, 5);
  });

  it("projects month-end from real extra-usage (linear from elapsed days)", () => {
    const e = computeEconomics(PLANS.max5, 1, month(100), NOW, 50);
    // ~21 of 31 days elapsed → projection ~50 * 31/21 ≈ 73.8
    expect(e.projectedExtra).toBeGreaterThan(70);
    expect(e.projectedExtra).toBeLessThan(78);
    expect(e.projectedTotal).toBeCloseTo(e.base + e.projectedExtra, 5);
  });
});

describe("computeEconomics — Team (seat-aware)", () => {
  it("clamps to the minimum seat count and prices per seat", () => {
    const e = computeEconomics(PLANS.team, 3, month(40), NOW);
    expect(e.seats).toBe(5); // min 5
    expect(e.base).toBe(150); // 5 × $30
  });

  it("scales the base with seat count", () => {
    const e = computeEconomics(PLANS.team, 12, month(40), NOW, 0);
    expect(e.seats).toBe(12);
    expect(e.base).toBe(360); // 12 × $30
    expect(e.extraUsage).toBe(0); // real extraUsage of 0 → no estimate
  });
});
