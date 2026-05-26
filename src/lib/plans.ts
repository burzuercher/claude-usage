import type { PlanTier, Month } from "./types";

// Plan budgets are no longer used to estimate usage percentages (that always
// comes from Anthropic's /api/oauth/usage). Plans now only contribute the
// subscription dollar base on the Spend tab. All values editable here.
export interface PlanDef {
  name: string;
  /** Flat monthly subscription price (USD). 0 for seat-priced plans. */
  monthlyBase: number;
  /** Per-seat monthly price (USD) for seat-priced plans (Team). */
  perSeat?: number;
  /** Minimum billable seats for seat-priced plans. */
  minSeats?: number;
}

export const PLANS: Record<PlanTier, PlanDef> = {
  pro: { name: "Pro", monthlyBase: 20 },
  max5: { name: "Max 5×", monthlyBase: 100 },
  max20: { name: "Max 20×", monthlyBase: 200 },
  team: {
    name: "Team",
    monthlyBase: 0,
    perSeat: 30, // standard Team seat, billed annually
    minSeats: 5,
  },
};

export const isSeatPriced = (plan: PlanDef) => (plan.perSeat ?? 0) > 0;

export interface Economics {
  /** Billable seats actually used (clamped to the plan minimum). */
  seats: number;
  /** Monthly subscription base (USD) — seats × perSeat for Team, else flat. */
  base: number;
  /** Extra-usage spend (USD) MTD. Real value from the API; 0 if unavailable. */
  extraUsage: number;
  /** Full API-rate value of this month's tracked tokens (reference, from local logs). */
  apiValue: number;
  /** base + extraUsage. */
  total: number;
  /** Projected extra-usage by month end (linear from current burn). */
  projectedExtra: number;
  /** Projected total (base + projectedExtra) by month end. */
  projectedTotal: number;
}

function daysInMonth(d: Date) {
  return new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
}

// Compute economics from real data only. `realExtraUsage` is the authoritative
// `extra_usage.used_credits` from Anthropic; pass undefined when unavailable
// (we then report 0 + no projection, never an estimate). `month` is local-log
// tracked usage, used only to surface the API-rate "value" of this month's
// tokens as an informational stat (not a usage cap).
export function computeEconomics(
  plan: PlanDef,
  seatsRaw: number,
  month: Month,
  now: number,
  realExtraUsage?: number
): Economics {
  const seats = isSeatPriced(plan) ? Math.max(plan.minSeats ?? 1, Math.floor(seatsRaw || 0)) : 1;
  const base = isSeatPriced(plan) ? seats * (plan.perSeat ?? 0) : plan.monthlyBase;

  const extraUsage = realExtraUsage ?? 0;
  const total = base + extraUsage;

  // Linear month-end projection from elapsed days — only meaningful when we have
  // real extra-usage; otherwise both projections collapse to the subscription base.
  let projectedExtra = 0;
  if (extraUsage > 0) {
    const nowDate = new Date(now);
    const elapsedDays = Math.max(0.5, (now - month.startedAt) / 86_400_000);
    projectedExtra = (extraUsage / elapsedDays) * daysInMonth(nowDate);
  }
  const projectedTotal = base + projectedExtra;

  return {
    seats,
    base,
    extraUsage,
    apiValue: month.cost,
    total,
    projectedExtra,
    projectedTotal,
  };
}
