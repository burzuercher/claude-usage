// Number / time formatting helpers (ported from the prototype's widget.jsx).

export const fmtNum = (n: number) => n.toLocaleString("en-US");

export const fmtTokens = (n: number) =>
  n >= 1e6 ? (n / 1e6).toFixed(2) + "M" : (n / 1e3).toFixed(1) + "k";

export const fmtDuration = (ms: number) => {
  if (ms < 0) ms = 0;
  const totalMin = Math.floor(ms / 60000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return h > 0 ? `${h}h ${String(m).padStart(2, "0")}m` : `${m}m`;
};

export const fmtClock = (d: Date) =>
  d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });

/** clamp 0..1 */
export const clamp01 = (n: number) => Math.max(0, Math.min(1, n));

/** "3h 47m" until the given ISO reset time. */
export const fmtResetRelative = (iso: string, now: number): string => {
  if (!iso) return "";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "";
  return fmtDuration(t - now);
};

/** Forecast minutes → readable "1h 54m" / "54m" / edge labels. */
export const fmtForecast = (min: number): string => {
  if (!Number.isFinite(min) || min >= 8 * 60) return "8h+";
  if (min < 1) return "under a minute";
  return fmtDuration(Math.round(min) * 60000);
};

/** "Fri 2:00 PM" absolute reset time. */
export const fmtResetAbsolute = (iso: string): string => {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleString("en-US", { weekday: "short", hour: "numeric", minute: "2-digit" });
};

/** "just now" / "3m ago" / "1h 04m ago" / "2d ago" for a past ms-epoch timestamp. */
export const fmtAgo = (ts: number, now: number): string => {
  if (!ts || !Number.isFinite(ts)) return "";
  const ms = Math.max(0, now - ts);
  if (ms < 60_000) return "just now";
  if (ms < 86_400_000) return `${fmtDuration(ms)} ago`;
  return `${Math.floor(ms / 86_400_000)}d ago`;
};

/** 0..1 share → "14%", with "<1%" for a non-zero sliver. */
export const fmtPct = (share: number): string => {
  if (!Number.isFinite(share) || share <= 0) return "0%";
  const pct = share * 100;
  return pct < 1 ? "<1%" : `${Math.round(pct)}%`;
};

/** "2d 4h" for spans of a day or more, else "5h 12m" / "12m". */
export const fmtSpan = (ms: number): string => {
  if (ms < 86_400_000) return fmtDuration(ms);
  const h = Math.floor(ms / 3_600_000);
  return `${Math.floor(h / 24)}d ${h % 24}h`;
};
