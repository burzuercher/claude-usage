import { useEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Icon } from "./Icon";
import { Overview, type Forecast, type WeeklyRow } from "./Overview";
import { Limits } from "./Limits";
import { Sessions } from "./Sessions";
import { fmtTokens, fmtForecast, fmtResetRelative, fmtResetAbsolute } from "../lib/format";
import type { Account, Attribution, RealUsage, UsageData } from "../lib/types";
import { computeEconomics, type PlanDef } from "../lib/plans";

// Best-effort window control — silently no-ops outside a Tauri context.
async function win<T>(fn: (w: ReturnType<typeof getCurrentWindow>) => Promise<T>) {
  try {
    await fn(getCurrentWindow());
  } catch {
    /* not running under Tauri */
  }
}

// Pin state persists across hide/show and restarts; off by default.
const PIN_KEY = "cu:pinned";
function loadPinned(): boolean {
  try {
    return localStorage.getItem(PIN_KEY) === "1";
  } catch {
    return false;
  }
}
// Tell the backend whether click-outside should dismiss the popover.
async function applyPinned(pinned: boolean) {
  try {
    await invoke("set_pinned", { pinned });
  } catch {
    /* not running under Tauri */
  }
}

/** Views in the body. Overview is live-API data; the other two are derived
 *  from local transcripts and only scan while they are on screen. */
export type Tab = "overview" | "limits" | "sessions";
const TABS: Tab[] = ["overview", "limits", "sessions"];

export function Widget({
  data,
  real,
  realLoading,
  attr,
  attrLoading,
  tab,
  onTabChange,
  plan,
  seats,
  compact,
  showSpend,
  settingsOpen,
  account,
  envLabel,
  envId,
  envCount,
  onOpenSettings,
  onRefresh,
}: {
  data: UsageData;
  real: RealUsage;
  realLoading: boolean;
  attr: Attribution;
  attrLoading: boolean;
  tab: Tab;
  onTabChange: (t: Tab) => void;
  plan: PlanDef;
  seats: number;
  compact: boolean;
  showSpend: boolean;
  /** Settings overlay is open — the window must be tall enough to show it. */
  settingsOpen: boolean;
  account?: Account;
  envLabel: string;
  envId: string;
  envCount: number;
  onOpenSettings: () => void;
  onRefresh: () => void;
}) {
  const [pinned, setPinned] = useState(loadPinned);
  const [now, setNow] = useState(Date.now());
  const winRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  // Sync the persisted pin state to the backend on mount so click-outside
  // behavior matches what the user last chose.
  useEffect(() => {
    applyPinned(pinned);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Auto-fit the OS window to the widget's content height so it never scrolls.
  // Content height is width-driven (not window-height driven), so resizing the
  // window can't feed back into the measurement.
  useEffect(() => {
    const el = winRef.current;
    if (!el) return;
    let raf = 0;
    let lastH = 0;
    let lastW = 0;
    const apply = () => {
      const rect = el.getBoundingClientRect();
      const w = Math.ceil(rect.width);
      const h = Math.ceil(rect.height);
      if (h <= 0 || (h === lastH && w === lastW)) return;
      lastH = h;
      lastW = w;
      win((wnd) => wnd.setSize(new LogicalSize(w, h)));
    };
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(apply);
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(raf);
    };
  }, []);

  // ── Live-only derivations: ring/weekly/forecast come from /api/oauth/usage
  //    (or Claude Code's cached copy of it), or are hidden when unavailable. ──
  const live = real.found && !!real.session;
  const sessionUsed = live ? real.session!.utilization : undefined;
  // A (cached) session whose reset time has passed describes a window that is
  // already over — label it instead of showing "resets in 0m".
  const sessionResetT = live && real.session!.resetsAt ? Date.parse(real.session!.resetsAt) : NaN;
  const sessionExpired = Number.isFinite(sessionResetT) && sessionResetT <= now;
  const resetIn = live ? (sessionExpired ? "a new window (data is stale)" : fmtResetRelative(real.session!.resetsAt, now)) : "";

  const weeklyRows: WeeklyRow[] = real.found
    ? real.weekly.map((b) => ({
        label: b.label,
        util: b.utilization,
        foot: b.resetsAt ? `resets ${fmtResetAbsolute(b.resetsAt)}` : "",
        note: b.note,
        active: b.isActive,
      }))
    : [];

  // ── Burn + forecast, all in the session window's frame of reference. ──
  // The local burn bins are indexed from the window start (live reset − 5h when
  // known). Utilization per token is calibrated from the window's own totals
  // (live % ÷ tokens logged in the window), so the pace of the last 30 minutes
  // can be projected in % terms. Falls back to the whole-window average pace
  // when the window isn't live-aligned or has too little data to calibrate.
  const BIN_MS = 15 * 60_000;
  const burnBins = data.burn.length || 20;
  const burnNow = Math.min(burnBins, Math.max(0, (now - data.session.startedAt) / BIN_MS));
  let burnSafe = 0; // safe tokens per bin (0 = unknown)
  let forecast: Forecast | undefined;
  if (live && !sessionExpired && Number.isFinite(sessionResetT)) {
    const resetT = sessionResetT;
    const sessionStart = resetT - 5 * 3600 * 1000;
    const elapsed = Math.max(60_000, now - sessionStart);
    const remaining = Math.max(0, resetT - now);
    const u = real.session!.utilization;

    let ratePerMs: number; // utilization per ms
    let basis: Forecast["basis"];
    const calibratable = data.session.fromLive && data.session.tokens >= 20_000 && u >= 0.02;
    if (calibratable) {
      const uPerToken = u / data.session.tokens;
      const nowIdx = Math.min(burnBins - 1, Math.floor(burnNow));
      const fromIdx = Math.max(0, nowIdx - 1); // previous bin + current partial bin ≈ last 30 min
      const recentTokens = data.burn.slice(fromIdx, nowIdx + 1).reduce((a, b) => a + b, 0);
      const recentMs = Math.max(60_000, now - (data.session.startedAt + fromIdx * BIN_MS));
      ratePerMs = (recentTokens / recentMs) * uPerToken;
      basis = "recent";
      burnSafe = u < 1 ? (1 - u) / uPerToken / Math.max(1, remaining / BIN_MS) : 0;
    } else {
      ratePerMs = u / elapsed;
      basis = "average";
    }

    if (u >= 1) {
      forecast = { basis, willBust: true, label: fmtForecast(0), projectedPct: 100 };
    } else if (ratePerMs <= 0) {
      forecast = { basis: "idle", willBust: false, label: "", projectedPct: Math.round(u * 100) };
    } else {
      const msToFull = (1 - u) / ratePerMs;
      forecast = {
        basis,
        willBust: msToFull < remaining,
        label: fmtForecast(msToFull / 60000),
        projectedPct: Math.round(Math.min(1, u + ratePerMs * remaining) * 100),
      };
    }
  }

  // Economics: real-data-driven. Extra usage is the authoritative dollar figure
  // from the API when available; the helper returns 0 / no projection otherwise.
  const econ = computeEconomics(plan, seats, data.month, now, real.extra?.usedDollars);
  const syncedAgo = Math.max(0, Math.floor((now - data.generatedAt) / 1000));

  // Footer: real extra-usage spend this billing month (what Anthropic actually
  // bills past the plan limits) — not an estimate.
  const extra = real.found ? real.extra : null;
  const extraValue = extra ? `$${extra.usedDollars.toFixed(2)}` : "—";
  const extraLabel = !extra
    ? "extra usage"
    : !extra.isEnabled
      ? "extra usage · off"
      : extra.limitDollars > 0
        ? `extra usage · of $${extra.limitDollars.toFixed(0)}`
        : "extra usage · month";
  const extraTitle = extra
    ? `Extra usage billed this month: $${extra.usedDollars.toFixed(2)} ${extra.currency}${extra.limitDollars > 0 ? ` of a $${extra.limitDollars.toFixed(2)} cap` : ""}${extra.disabledReason ? ` · ${extra.disabledReason}` : ""}`
    : "Extra-usage spend appears here once live data is available";

  const togglePin = async () => {
    const next = !pinned;
    setPinned(next);
    try {
      localStorage.setItem(PIN_KEY, next ? "1" : "0");
    } catch {
      /* storage unavailable */
    }
    await applyPinned(next);
  };

  return (
    <div ref={winRef} className={`win ${compact ? "compact" : ""} ${settingsOpen ? "settings-open" : ""}`}>
      {/* Title bar — Windows 11 style (drag region) */}
      <div className="titlebar" data-tauri-drag-region>
        <div className="title-left" data-tauri-drag-region>
          <div className="app-mark">
            <svg viewBox="0 0 24 24" width="14" height="14">
              <path d="M12 3v18M5 7l14 10M19 7L5 17M3 12h18" stroke="var(--clay)" strokeWidth="2" strokeLinecap="round" />
            </svg>
          </div>
          <div className="title-text" data-tauri-drag-region>
            <b>
              Claude Usage
              {data.isMock && <span className="mock-badge">demo</span>}
              {live && real.source === "api" && <span className="live-badge"><span className="live-dot" />live</span>}
              {live && real.source === "cache" && <span className="live-badge cached" title="Showing Claude Code's cached usage"><span className="live-dot" />cached</span>}
            </b>
            <span className="title-sub" title={`${envId || "local"} · synced ${syncedAgo}s ago`}>
              {(account?.orgName || account?.email || envLabel || "Local logs")} · {plan.name}
              {/* Show dir-id when multiple envs exist so the user knows which one is selected,
                  especially because .claude is Claude Code's currently active login (volatile). */}
              {envCount > 1 && envId && <span className="title-env"> · {envId}</span>}
            </span>
          </div>
        </div>
        <div className="title-right">
          <button className="chrome-btn tool" title="Settings" onClick={onOpenSettings}>
            <Icon name="gear" size={13} />
          </button>
          <button className="chrome-btn tool" title="Refresh" onClick={onRefresh}>
            <Icon name="refresh" size={13} />
          </button>
          <button className={`chrome-btn pin ${pinned ? "on" : ""}`} onClick={togglePin} title={pinned ? "Pinned — stays open (click to unpin)" : "Pin — keep open when clicking away"}>
            <Icon name="pin" />
          </button>
          <button className="chrome-btn" title="Minimize" onClick={() => win((w) => w.minimize())}>
            <Icon name="min" />
          </button>
          <button className="chrome-btn" title="Maximize" onClick={() => win((w) => w.toggleMaximize())}>
            <Icon name="max" />
          </button>
          <button className="chrome-btn close" title="Close" onClick={() => win((w) => w.close())}>
            <Icon name="x" />
          </button>
        </div>
      </div>

      <div className="tabs">
        {TABS.map((id) => (
          <button key={id} className={`tab ${tab === id ? "on" : ""}`} onClick={() => onTabChange(id)}>
            {id}
          </button>
        ))}
      </div>

      <div className="body">
        {tab === "limits" ? (
          <Limits attr={attr} loading={attrLoading} />
        ) : tab === "sessions" ? (
          <Sessions attr={attr} loading={attrLoading} now={now} />
        ) : (
          <Overview
            sessionUsed={sessionUsed}
            sessionNote={live ? real.session!.note : ""}
            resetIn={resetIn}
            live={live}
            source={real.source}
            fetchedAt={real.fetchedAt}
            now={now}
            realLoading={realLoading}
            unavailableReason={real.reason}
            weeklyRows={weeklyRows}
            forecast={forecast}
            models={data.models}
            sessionFromLive={data.session.fromLive}
            burn={data.burn}
            burnNow={burnNow}
            burnSafe={burnSafe}
            econ={econ}
            extra={real.extra}
            showSpend={showSpend}
          />
        )}
      </div>

      <div className="footer">
        <div className="footer-stat">
          <span className="footer-num mono">{fmtTokens(data.today.tokens)}</span>
          <span className="footer-lbl">tokens today</span>
        </div>
        <div className="footer-divider" />
        <div className="footer-stat" title={extraTitle}>
          <span className={`footer-num mono ${real.extra && real.extra.usedDollars > 0 ? "hot" : ""}`}>{extraValue}</span>
          <span className="footer-lbl">{extraLabel}</span>
        </div>
        <div className="footer-divider" />
        <div className="footer-stat">
          <span className="footer-num mono">{data.today.prompts}</span>
          <span className="footer-lbl">prompts</span>
        </div>
      </div>
    </div>
  );
}
