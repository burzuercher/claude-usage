import type { PlanTier, Month } from "./types";

// Plan budgets (prompts, per seat) and subscription prices. These are estimates
// — Anthropic does not expose Pro/Max/Team session or weekly caps via a public
// API — so they're used to turn raw prompt counts into progress-bar percentages
// and to estimate expenditures. All values are editable here.
export interface PlanDef {
  name: string;
  sessionBudget: number; // prompts per 5h block (per seat)
  weeklyBudget: number; // weekly cap, all models (per seat)
  weeklyOpusBudget: number;
  /** Flat monthly subscription price (USD). 0 for seat-priced plans. */
  monthlyBase: number;
  /** Per-seat monthly price (USD) for seat-priced plans (Team). */
  perSeat?: number;
  /** Minimum billable seats for seat-priced plans. */
  minSeats?: number;
}

export const PLANS: Record<PlanTier, PlanDef> = {
  pro: {
    name: "Pro",
    sessionBudget: 45,
    weeklyBudget: 480,
    weeklyOpusBudget: 96,
    monthlyBase: 20,
  },
  max5: {
    name: "Max 5×",
    sessionBudget: 225,
    weeklyBudget: 2400,
    weeklyOpusBudget: 480,
    monthlyBase: 100,
  },
  max20: {
    name: "Max 20×",
    sessionBudget: 900,
    weeklyBudget: 9600,
    weeklyOpusBudget: 1920,
    monthlyBase: 200,
  },
  team: {
    name: "Team",
    // Per-seat budgets (~2× Pro); each member gets their own allowance.
    sessionBudget: 90,
    weeklyBudget: 960,
    weeklyOpusBudget: 192,
    monthlyBase: 0,
    perSeat: 30, // standard Team seat, billed annually
    minSeats: 5,
  },
};

export const isSeatPriced = (plan: PlanDef) => (plan.perSeat ?? 0) > 0;

// Average days per month, for converting weekly budgets to a monthly allowance.
const DAYS_PER_MONTH = 365.25 / 12;

export interface Economics {
  /** Billable seats actually used (clamped to the plan minimum). */
  seats: number;
  /** Monthly subscription base (USD) — seats × perSeat for Team, else flat. */
  base: number;
  /** Estimated prompts the subscription includes this month (this seat). */
  monthPromptBudget: number;
  /** Estimated extra-usage (overage past limits) billed at API rates, MTD. */
  extraUsage: number;
  /** Full API-rate value of this month's usage (reference). */
  apiValue: number;
  /** base + extraUsage, month to date. */
  total: number;
  /** Projected extra usage by month end (linear from current burn). */
  projectedExtra: number;
  /** Projected total (base + projectedExtra) by month end. */
  projectedTotal: number;
  /** 0..1 fraction of the monthly prompt budget consumed (this seat). */
  budgetUsed: number;
}

function daysInMonth(d: Date) {
  return new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
}

// Estimate this month's subscription + extra-usage spend from real usage.
// Extra usage is modelled as the API-rate cost of the prompts that exceeded the
// plan's included monthly allowance (prorated from real cost-per-prompt).
export function computeEconomics(
  plan: PlanDef,
  seatsRaw: number,
  month: Month,
  now: number
): Economics {
  const seats = isSeatPriced(plan) ? Math.max(plan.minSeats ?? 1, Math.floor(seatsRaw || 0)) : 1;
  const base = isSeatPriced(plan) ? seats * (plan.perSeat ?? 0) : plan.monthlyBase;

  const monthPromptBudget = plan.weeklyBudget * (DAYS_PER_MONTH / 7);
  const excess = Math.max(0, month.prompts - monthPromptBudget);
  const extraFraction = month.prompts > 0 ? excess / month.prompts : 0;
  const extraUsage = extraFraction * month.cost;

  const total = base + extraUsage;

  // Linear month-end projection from elapsed days.
  const nowDate = new Date(now);
  const elapsedDays = Math.max(0.5, (now - month.startedAt) / 86_400_000);
  const projectedExtra = (extraUsage / elapsedDays) * daysInMonth(nowDate);
  const projectedTotal = base + projectedExtra;

  return {
    seats,
    base,
    monthPromptBudget,
    extraUsage,
    apiValue: month.cost,
    total,
    projectedExtra,
    projectedTotal,
    budgetUsed: monthPromptBudget > 0 ? Math.min(1, month.prompts / monthPromptBudget) : 0,
  };
}
