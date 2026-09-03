import { useCountUp } from "../hooks/useCountUp";
import { Ring } from "./Ring";
import { StackedBar } from "./StackedBar";
import { BurnSpark } from "./BurnSpark";
import { Icon } from "./Icon";
import { fmtAgo } from "../lib/format";
import { MODEL_COLOR, type ExtraUsage, type ModelStat, type RealSource } from "../lib/types";
import type { Economics } from "../lib/plans";

export interface WeeklyRow {
  label: string;
  util: number; // 0..1
  foot: string; // right-aligned note (reset time)
  note: string; // left-aligned promo / boost notice, may be ""
  active: boolean; // server says this window is the binding one
}

export interface Forecast {
  /** "recent" = last-30-min token burn calibrated to live %; "average" = whole-window average; "idle" = no recent activity. */
  basis: "recent" | "average" | "idle";
  willBust: boolean;
  /** Time until 100% at this pace ("1h 24m", "8h+"), "" when idle. */
  label: string;
  /** Projected utilization at reset, 0..100. */
  projectedPct: number;
}

export interface OverviewProps {
  /** Real session utilization (0..1) from /api/oauth/usage, undefined when unavailable. */
  sessionUsed: number | undefined;
  /** Promo / boost notice attached to the session bar, may be "". */
  sessionNote: string;
  resetIn: string;
  /** True when live data (API or Claude Code's cache) has produced a session result. */
  live: boolean;
  source: RealSource;
  /** ms epoch the live data was fetched from Anthropic. */
  fetchedAt: number;
  now: number;
  /** True while the first live call for this env is in flight. */
  realLoading: boolean;
  /** Reason live is unavailable / degraded (from the backend). */
  unavailableReason?: string;
  weeklyRows: WeeklyRow[]; // empty when no live data
  forecast?: Forecast;
  models: ModelStat[];
  /** True when the model split / burn window came from the live reset time. */
  sessionFromLive: boolean;
  /** Tokens per 15-min bin from the session window start. */
  burn: number[];
  /** Bins elapsed so far (fractional). */
  burnNow: number;
  /** Safe tokens per bin to land exactly at 100% at reset (0 = unknown). */
  burnSafe: number;
  econ: Economics;
  extra: ExtraUsage | null;
  showSpend: boolean;
}

/** Live data older than this is flagged as stale. */
const STALE_MS = 10 * 60_000;

// Translate the backend's terse `reason` strings into actionable user guidance.
function reasonHint(reason: string | undefined): string {
  const r = (reason ?? "").toLowerCase();
  if (!r) return "";
  if (r.includes("no credentials")) return "Sign in to this account in Claude Code to enable live limits.";
  if (r.includes("token expired")) return "Token expired — run `claude` in this account to refresh.";
  if (r.includes("401")) return "Token rejected by Anthropic — sign in again in Claude Code.";
  if (r.includes("429")) return "Anthropic is rate-limiting usage checks — retrying with backoff.";
  if (r.includes("request failed")) return "Couldn't reach the Claude API — check your network.";
  if (r.includes("can't read") || r.includes("bad credentials")) return "Credentials file is unreadable — try signing in again.";
  return "";
}

const usd = (n: number) => `$${n.toFixed(2)}`;

function AnimatedWeekly({ row, ink }: { row: WeeklyRow; ink: boolean }) {
  const anim = useCountUp(row.util * 100, 900);
  return (
    <div className="weekly-row">
      <div className="weekly-head">
        <span className="weekly-lbl">{row.label}</span>
        <span className="weekly-val mono">{Math.round(anim)}%</span>
      </div>
      <div className="bar">
        <div className={`bar-fill ${ink ? "ink" : "clay"}`} style={{ width: `${Math.min(100, anim)}%` }} />
      </div>
      <div className="weekly-foot">
        <span className="weekly-note" title={row.note}>{row.note}</span>
        <span className="mono">{row.foot}</span>
      </div>
    </div>
  );
}

export function Overview(p: OverviewProps) {
  const totalTokens = Math.max(1, p.models.reduce((a, m) => a + m.tokens, 0));
  const segments = p.models.map((m) => ({
    id: m.id,
    share: m.tokens / totalTokens,
    color: MODEL_COLOR[m.id] ?? "var(--moss)",
  }));

  const age = p.fetchedAt ? p.now - p.fetchedAt : 0;
  const stale = p.live && age > STALE_MS;
  const degraded = p.live && !!p.unavailableReason; // e.g. serving cache because the API 429'd
  const sourceLabel = p.source === "cache" ? "via Claude Code cache" : "live from Claude";

  return (
    <>
      {/* Live-only: ring, forecast, weekly limits. Shows a loading placeholder
          during the initial fetch for an env (so switching accounts doesn't
          flash the empty state), then either the data or an honest empty state
          with reason-specific guidance. */}
      {p.live && p.sessionUsed !== undefined ? (
        <>
          <Ring value={p.sessionUsed} resetIn={p.resetIn} />
          {p.sessionNote && <div className="ring-note">{p.sessionNote}</div>}

          {p.forecast && (
            <div className={`forecast ${p.forecast.willBust ? "warn" : ""}`}>
              <Icon name="spark" size={13} />
              {p.forecast.basis === "idle" ? (
                <span>
                  No activity in the last 30 min · on track for <b className="mono">{p.forecast.projectedPct}%</b> at reset
                </span>
              ) : p.forecast.willBust ? (
                <span>
                  At the {p.forecast.basis === "recent" ? "last 30 min" : "session's average"} pace, you'll{" "}
                  <b>hit the cap</b> in <b className="mono">{p.forecast.label}</b>
                </span>
              ) : (
                <span>
                  At the {p.forecast.basis === "recent" ? "last 30 min" : "session's average"} pace, you'll{" "}
                  <b>finish under cap</b> · ~<b className="mono">{p.forecast.projectedPct}%</b> at reset
                </span>
              )}
            </div>
          )}

          {p.weeklyRows.length > 0 && (
            <div className="weekly">
              {p.weeklyRows.map((r, i) => (
                <AnimatedWeekly key={r.label} row={r} ink={i > 0 && !r.active} />
              ))}
            </div>
          )}

          <div
            className={`weekly-src ${stale ? "stale" : ""}`}
            title={degraded ? `${p.unavailableReason} — ${reasonHint(p.unavailableReason)}` : undefined}
          >
            <span className="live-dot" /> {sourceLabel} · updated {fmtAgo(p.fetchedAt, p.now) || "just now"}
            {stale && " · stale"}
            {degraded && !stale && " · retrying"}
          </div>
        </>
      ) : p.realLoading ? (
        <div className="empty-hint loading">
          <span className="spinner" />
          Connecting to Claude…
        </div>
      ) : (
        <div className="empty-hint">
          Live usage limits aren't available for this account
          {p.unavailableReason ? <> · <span className="mono">{p.unavailableReason}</span></> : null}.
          {(() => {
            const hint = reasonHint(p.unavailableReason);
            return hint ? (<><br />{hint}</>) : null;
          })()}
        </div>
      )}

      {/* Expenditure mini card — surface only when enabled in settings */}
      {p.showSpend && (
        <div className="block spend-mini">
          <div className="block-head">
            <span className="block-title">Expenditure · this month</span>
            <span className="block-meta mono">~{usd(p.econ.projectedTotal)} proj.</span>
          </div>
          <div className="spend-mini-row">
            <div className="spend-mini-cell">
              <span className="spend-mini-num mono">{usd(p.econ.total)}</span>
              <span className="spend-mini-lbl">so far</span>
            </div>
            <div className="spend-mini-cell">
              <span className="spend-mini-num mono">{usd(p.econ.base)}</span>
              <span className="spend-mini-lbl">subscription</span>
            </div>
            <div className="spend-mini-cell">
              <span className={`spend-mini-num mono ${p.econ.extraUsage > 0 ? "hot" : ""}`}>{usd(p.econ.extraUsage)}</span>
              <span className="spend-mini-lbl">
                {p.extra && !p.extra.isEnabled
                  ? "credits off"
                  : p.extra && p.extra.limitDollars > 0
                    ? `extra · of ${usd(p.extra.limitDollars)}`
                    : "extra usage"}
              </span>
            </div>
          </div>
        </div>
      )}

      {/* Local-source factual data (not estimates) */}
      <div className="block">
        <div className="block-head">
          <span className="block-title">This session · models</span>
          {!p.sessionFromLive && <span className="block-meta">local 5h block</span>}
        </div>
        {segments.length > 0 ? (
          <>
            <StackedBar segments={segments} height={8} />
            <div className="legend">
              {segments.map((s) => {
                const m = p.models.find((x) => x.id === s.id)!;
                return (
                  <div className="legend-item" key={s.id}>
                    <span className="legend-dot" style={{ background: s.color }} />
                    <span className="legend-name">{m.name}</span>
                    <span className="legend-val mono">{Math.round(s.share * 100)}%</span>
                  </div>
                );
              })}
            </div>
          </>
        ) : (
          <div className="block-meta">No turns logged in this session yet.</div>
        )}
      </div>

      <div className="block">
        <div className="block-head">
          <span className="block-title">Burn rate · this session</span>
          <span className="block-meta" title="Tokens per 15 minutes across the 5-hour window. The dashed line is the pace that lands exactly at 100% when the window resets.">
            {p.burnSafe > 0 ? (
              <>
                <span className="dot" style={{ background: "transparent", border: "1px dashed var(--ink-40)" }} /> safe = 100% at reset
              </>
            ) : (
              <>tokens / 15 min</>
            )}
          </span>
        </div>
        <BurnSpark data={p.burn} threshold={p.burnSafe} nowBins={p.burnNow} />
      </div>
    </>
  );
}
