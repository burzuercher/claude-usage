import { useEffect, useState, type ReactNode } from "react";
import { fmtAgo, fmtPct, fmtResetAbsolute, fmtResetRelative, fmtSpan } from "../lib/format";
import { allModelsBucket, sessionPace, weeklyPace, type WeeklyPace } from "../lib/pace";
import type { RealBucket, RealUsage, UsageData } from "../lib/types";

/** Mirrors `panel::snapshot` in the Rust backend. */
interface Snapshot {
  envId: string;
  accountLabel: string;
  real: RealUsage;
  usage: UsageData;
  generatedAt: number;
}

/** The app memoizes its own polling, so asking often costs nothing upstream. */
const POLL_MS = 30_000;
/** Live data older than this is flagged as stale (same as the widget). */
const STALE_MS = 10 * 60_000;

// Same-origin when served by the app; `?api=http://127.0.0.1:47821` when
// developing against Vite's dev server.
const API = new URLSearchParams(location.search).get("api") ?? "";

function useSnapshot() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    const run = async () => {
      try {
        const res = await fetch(`${API}/api/snapshot`, { cache: "no-store" });
        if (!res.ok) throw new Error(String(res.status));
        const s = (await res.json()) as Snapshot;
        if (!cancelled) {
          setSnap(s);
          setError(false);
        }
      } catch {
        // Keep showing the last snapshot; just flag that it isn't refreshing.
        if (!cancelled) setError(true);
      }
      if (!cancelled) timer = window.setTimeout(run, POLL_MS);
    };
    void run();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, []);
  return { snap, error };
}

const level = (u: number) => (u >= 0.85 ? "hot" : u >= 0.6 ? "warm" : "");

function Bar({ util }: { util: number }) {
  return (
    <div className="p-bar">
      <div className={`p-fill ${level(util)}`} style={{ width: `${Math.min(100, util * 100)}%` }} />
    </div>
  );
}

function Row({ k, v, warn }: { k: string; v: ReactNode; warn?: boolean }) {
  return (
    <div className={`p-kv ${warn ? "warn" : ""}`}>
      <span>{k}</span>
      <span className="p-mono">{v}</span>
    </div>
  );
}

function WeeklyPaceRows({ pace }: { pace: WeeklyPace }) {
  if (pace.basis === "early") {
    return (
      <>
        <Row k={`Since reset (${fmtSpan(pace.sinceStart)})`} v={fmtPct(pace.perDay)} />
        <Row k="Budget" v={`${fmtPct(pace.budgetPerDay)}/day`} />
      </>
    );
  }
  const capAt = new Date(pace.capAt).toISOString();
  return (
    <>
      <Row k={pace.basis === "day" ? "Last 24h" : "Avg per day"} v={`${fmtPct(pace.perDay)}/day`} warn={pace.willBust} />
      <Row k="Budget" v={`${fmtPct(pace.budgetPerDay)}/day`} />
      <Row
        k="At this pace"
        v={pace.willBust ? `cap ${fmtResetAbsolute(capAt)}` : `~${pace.projectedPct}% at reset`}
        warn={pace.willBust}
      />
    </>
  );
}

function WeeklyBlock({ b, pace, now }: { b: RealBucket; pace?: WeeklyPace; now: number }) {
  const left = b.resetsAt ? Date.parse(b.resetsAt) - now : NaN;
  return (
    <section className="p-block">
      <div className="p-head">
        <span className="p-label">{b.label === "All models" ? "Week · all models" : b.label}</span>
        <span className="p-pct p-mono">{Math.round(b.utilization * 100)}%</span>
      </div>
      <Bar util={b.utilization} />
      {b.resetsAt && (
        <div className="p-foot">
          <span>resets {fmtResetAbsolute(b.resetsAt)}</span>
          {Number.isFinite(left) && left > 0 && <span className="p-mono">{fmtSpan(left)}</span>}
        </div>
      )}
      {b.note && <div className="p-note">{b.note}</div>}
      {pace && <WeeklyPaceRows pace={pace} />}
    </section>
  );
}

export function Panel() {
  const { snap, error } = useSnapshot();
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 15_000);
    return () => clearInterval(id);
  }, []);

  if (!snap) {
    return (
      <div className="p-root">
        <div className="p-empty">{error ? "Claude Usage isn't responding. Is the app running?" : "Connecting…"}</div>
      </div>
    );
  }

  const { real, usage } = snap;
  const live = real.found && !!real.session;
  const session = real.session;
  const sessionReset = session?.resetsAt ? Date.parse(session.resetsAt) : NaN;
  const sessionOver = Number.isFinite(sessionReset) && sessionReset <= now;
  const { forecast } = sessionPace(real, usage, now);
  const allModels = allModelsBucket(real);
  const wPace = weeklyPace(allModels, usage.week, now);
  const stale = live && now - real.fetchedAt > STALE_MS;
  const extra = real.extra;

  return (
    <div className="p-root">
      <header className="p-title">
        <span className="p-name">Claude Usage</span>
        <span className={`p-status ${stale || error ? "stale" : ""}`} title={real.reason || undefined}>
          <span className="p-dot" />
          {live ? fmtAgo(real.fetchedAt, now) : "offline"}
        </span>
      </header>
      {(snap.accountLabel || snap.envId) && (
        <div className="p-sub">
          {snap.accountLabel}
          {snap.envId && snap.envId !== ".claude" ? ` · ${snap.envId}` : ""}
        </div>
      )}

      {!live || !session ? (
        <div className="p-empty">
          Live limits aren't available{real.reason ? ` · ${real.reason}` : ""}.
        </div>
      ) : (
        <>
          <section className="p-block">
            <div className="p-head">
              <span className="p-label">Session · 5h</span>
              <span className="p-pct p-mono">{Math.round(session.utilization * 100)}%</span>
            </div>
            <Bar util={session.utilization} />
            <div className="p-foot">
              {sessionOver ? (
                <span>window reset · refreshing</span>
              ) : (
                <>
                  <span>resets in {fmtResetRelative(session.resetsAt, now)}</span>
                  <span className="p-mono">{fmtResetAbsolute(session.resetsAt).replace(/^\w+ /, "")}</span>
                </>
              )}
            </div>
            {session.note && <div className="p-note">{session.note}</div>}
            {forecast &&
              (forecast.basis === "idle" ? (
                <Row k="Pace (30m)" v={`idle · ~${forecast.projectedPct}%`} />
              ) : (
                <Row
                  k={forecast.basis === "recent" ? "Pace (30m)" : "Pace (avg)"}
                  v={forecast.willBust ? `cap in ${forecast.label}` : `~${forecast.projectedPct}% at reset`}
                  warn={forecast.willBust}
                />
              ))}
          </section>

          {real.weekly.map((b) => (
            <WeeklyBlock key={b.key || b.label} b={b} pace={b === allModels ? wPace : undefined} now={now} />
          ))}

          {extra && (extra.isEnabled || extra.usedDollars > 0) && (
            <section className="p-block">
              <Row
                k="Extra usage"
                v={`$${extra.usedDollars.toFixed(2)}${extra.limitDollars > 0 ? ` / $${extra.limitDollars.toFixed(0)}` : ""}`}
                warn={extra.spendLimitReached}
              />
            </section>
          )}
        </>
      )}
    </div>
  );
}
