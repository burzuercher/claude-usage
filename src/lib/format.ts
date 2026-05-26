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
