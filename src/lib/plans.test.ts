import { describe, it, expect } from "vitest";
import { PLANS, isSeatPriced, computeEconomics } from "./plans";
import type { Month } from "./types";

const NOW = Date.UTC(2026, 4, 21); // May 21 2026
const monthStart = Date.UTC(2026, 4, 1); // May 1 2026

const month = (prompts: number, cost: number): Month => ({
  startedAt: monthStart,
  tokens: 0,
  cost,
  prompts,
});

describe("isSeatPriced", () => {
  it("is true only for seat-priced plans", () => {
    expect(isSeatPriced(PLANS.team)).toBe(true);
    expect(isSeatPriced(PLANS.pro)).toBe(false);
    expect(isSeatPriced(PLANS.max20)).toBe(false);
  });
});

describe("computeEconomics — flat plans", () => {
  it("uses the flat monthly base and no overage under budget", () => {
    const e = computeEconomics(PLANS.pro, 1, month(1000, 50), NOW);
    expect(e.base).toBe(20);
    expect(e.extraUsage).toBe(0);
    expect(e.total).toBe(20);
    expect(e.apiValue).toBe(50);
    expect(e.budgetUsed).toBeGreaterThan(0);
    expect(e.budgetUsed).toBeLessThan(1);
  });

  it("bills overage past the monthly allowance at API rates", () => {
    const e = computeEconomics(PLANS.pro, 1, month(4000, 100), NOW);
    // monthly budget ≈ 480 * 365.25/12/7 ≈ 2087 prompts
    expect(e.monthPromptBudget).toBeCloseTo(2087.1, 0);
    expect(e.extraUsage).toBeGreaterThan(40);
    expect(e.extraUsage).toBeLessThan(55);
    expect(e.total).toBeCloseTo(e.base + e.extraUsage, 5);
    expect(e.budgetUsed).toBe(1); // clamped
  });

  it("projects month-end above current when over budget", () => {
    const e = computeEconomics(PLANS.pro, 1, month(4000, 100), NOW);
    expect(e.projectedExtra).toBeGreaterThan(e.extraUsage);
    expect(e.projectedTotal).toBeGreaterThan(e.total);
  });
});

describe("computeEconomics — Team (seat-aware)", () => {
  it("clamps to the minimum seat count and prices per seat", () => {
    const e = computeEconomics(PLANS.team, 3, month(1000, 40), NOW);
    expect(e.seats).toBe(5); // min 5
    expect(e.base).toBe(150); // 5 × $30
  });

  it("scales the base with seat count", () => {
    const e = computeEconomics(PLANS.team, 12, month(1000, 40), NOW);
    expect(e.seats).toBe(12);
    expect(e.base).toBe(360); // 12 × $30
  });
});
