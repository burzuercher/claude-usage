// Pace math shared by the widget and the iTerm2 panel: how fast the live
// limits are filling and where that pace lands at reset.
//
// Both paces calibrate local transcript usage against the live % from
// Anthropic: live % ÷ local usage over the whole window gives "% per unit of
// usage", which turns recent local usage into a % rate. Only the ratio matters,
// so the absolute weighting (tokens for the session, estimated cost for the
// week) never shows up in the numbers.

import { fmtForecast } from "./format";
import type { RealBucket, RealUsage, UsageData, Week } from "./types";

const MIN = 60_000;
const DAY = 86_400_000;
const BIN_MS = 15 * MIN;

export interface Forecast {
  /** "recent" = last-30-min token burn calibrated to live %; "average" = whole-window average; "idle" = no recent activity. */
  basis: "recent" | "average" | "idle";
  willBust: boolean;
  /** Time until 100% at this pace ("1h 24m", "8h+"), "" when idle. */
  label: string;
  /** Projected utilization at reset, 0..100. */
  projectedPct: number;
}

export interface SessionPace {
  forecast?: Forecast;
  /** Safe tokens per 15-min bin to land exactly at 100% at reset (0 = unknown). */
  burnSafe: number;
  /** Burn bins elapsed so far (fractional). */
  burnNow: number;
}

/** 5-hour session pace. The local burn bins are indexed from the window start
 *  (live reset − 5h when known). Utilization per token is calibrated from the
 *  window's own totals, so the last 30 minutes can be projected in % terms;
 *  falls back to the whole-window average when the window isn't live-aligned or
 *  has too little data to calibrate. */
export function sessionPace(real: RealUsage, data: UsageData, now: number): SessionPace {
  const burnBins = data.burn.length || 20;
  const burnNow = Math.min(burnBins, Math.max(0, (now - data.session.startedAt) / BIN_MS));
  const live = real.found && !!real.session;
  const resetT = live && real.session!.resetsAt ? Date.parse(real.session!.resetsAt) : NaN;
  if (!live || !Number.isFinite(resetT) || resetT <= now) return { burnSafe: 0, burnNow };

  const sessionStart = resetT - 5 * 3600 * 1000;
  const elapsed = Math.max(MIN, now - sessionStart);
  const remaining = Math.max(0, resetT - now);
  const u = real.session!.utilization;

  let burnSafe = 0;
  let ratePerMs: number; // utilization per ms
  let basis: Forecast["basis"];
  const calibratable = data.session.fromLive && data.session.tokens >= 20_000 && u >= 0.02;
  if (calibratable) {
    const uPerToken = u / data.session.tokens;
    const nowIdx = Math.min(burnBins - 1, Math.floor(burnNow));
    const fromIdx = Math.max(0, nowIdx - 1); // previous bin + current partial bin ≈ last 30 min
    const recentTokens = data.burn.slice(fromIdx, nowIdx + 1).reduce((a, b) => a + b, 0);
    const recentMs = Math.max(MIN, now - (data.session.startedAt + fromIdx * BIN_MS));
    ratePerMs = (recentTokens / recentMs) * uPerToken;
    basis = "recent";
    burnSafe = u < 1 ? (1 - u) / uPerToken / Math.max(1, remaining / BIN_MS) : 0;
  } else {
    ratePerMs = u / elapsed;
    basis = "average";
  }

  let forecast: Forecast;
  if (u >= 1) {
    forecast = { basis, willBust: true, label: fmtForecast(0), projectedPct: 100 };
  } else if (ratePerMs <= 0) {
    forecast = { basis: "idle", willBust: false, label: "", projectedPct: Math.round(u * 100) };
  } else {
    const msToFull = (1 - u) / ratePerMs;
    forecast = {
      basis,
      willBust: msToFull < remaining,
      label: fmtForecast(msToFull / MIN),
      projectedPct: Math.round(Math.min(1, u + ratePerMs * remaining) * 100),
    };
  }
  return { forecast, burnSafe, burnNow };
}

export interface WeeklyPace {
  /** "day" = the last 24h of local usage, calibrated to the live weekly %;
   *  "average" = the window's average per day (local logs couldn't be calibrated);
   *  "early" = the window is under a day old — too short to project, so only
   *  the share used since reset and the daily budget are meaningful. */
  basis: "day" | "average" | "early";
  /** Share of the weekly limit used per day at this pace (for "early": used since reset). */
  perDay: number;
  /** Share per day that lands exactly at 100% at reset. */
  budgetPerDay: number;
  willBust: boolean;
  /** ms epoch this pace reaches the cap (0 when it doesn't before reset). */
  capAt: number;
  /** Projected utilization at reset, 0..100. */
  projectedPct: number;
  /** ms epoch of the weekly reset. */
  resetAt: number;
  /** ms since the weekly window started. */
  sinceStart: number;
}

/** The weekly bucket the daily pace applies to: "All models". Per-model buckets
 *  would need model-scoped local usage to calibrate, so they get no pace. */
export function allModelsBucket(real: RealUsage): RealBucket | undefined {
  return real.found ? real.weekly.find((b) => b.key === "seven_day") : undefined;
}

/** Daily pace for a weekly window: what a whole day at the current rate means
 *  for the 7-day limit. Deliberately a day, not the last half hour — a burst in
 *  one session says little about the week. */
export function weeklyPace(bucket: RealBucket | undefined, week: Week, now: number): WeeklyPace | undefined {
  const resetAt = bucket?.resetsAt ? Date.parse(bucket.resetsAt) : NaN;
  if (!bucket || !Number.isFinite(resetAt) || resetAt <= now) return undefined;

  const start = resetAt - 7 * DAY;
  const sinceStart = Math.max(MIN, now - start);
  const remaining = resetAt - now;
  const u = bucket.utilization;
  const budgetPerDay = u >= 1 ? 0 : (1 - u) / (remaining / DAY);
  const base = { budgetPerDay, resetAt, sinceStart };

  if (sinceStart < DAY) {
    return { ...base, basis: "early", perDay: u, willBust: u >= 1, capAt: u >= 1 ? now : 0, projectedPct: Math.round(u * 100) };
  }

  // The local totals must describe this same window to be calibrated against it.
  const aligned = week.fromLive && Math.abs(week.startedAt - start) < 5 * MIN && week.cost > 0 && u >= 0.01;
  const basis: WeeklyPace["basis"] = aligned ? "day" : "average";
  const perDay = aligned ? u * (week.cost24h / week.cost) : u / (sinceStart / DAY);

  const projected = u + perDay * (remaining / DAY);
  const willBust = u >= 1 || projected >= 1;
  const capAt = !willBust ? 0 : u >= 1 ? now : now + ((1 - u) / perDay) * DAY;
  return { ...base, basis, perDay, willBust, capAt, projectedPct: Math.round(Math.min(1, projected) * 100) };
}
