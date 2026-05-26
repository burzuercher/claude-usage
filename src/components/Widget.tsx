import { useEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Icon } from "./Icon";
import { OverviewTab } from "./tabs/OverviewTab";
import { ModelsTab } from "./tabs/ModelsTab";
import { SurfacesTab } from "./tabs/SurfacesTab";
import { SpendTab } from "./tabs/SpendTab";
import { HistoryTab } from "./tabs/HistoryTab";
import { fmtTokens, fmtDuration, fmtForecast, fmtResetRelative, fmtResetAbsolute, clamp01 } from "../lib/format";
import type { Account, RealUsage, UsageData } from "../lib/types";
import type { WeeklyRow } from "./tabs/OverviewTab";
import { computeEconomics, type PlanDef } from "../lib/plans";

type Tab = "overview" | "models" | "surfaces" | "spend" | "history";
const TABS: Tab[] = ["overview", "models", "surfaces", "spend", "history"];

// Best-effort window control — silently no-ops outside a Tauri context.
async function win<T>(fn: (w: ReturnType<typeof getCurrentWindow>) => Promise<T>) {
  try {
    await fn(getCurrentWindow());
  } catch {
    /* not running under Tauri */
  }
}

export function Widget({
  data,
  real,
  plan,
  seats,
  compact,
  showSpend,
  account,
  envLabel,
  envId,
  envCount,
  onOpenSettings,
  onRefresh,
}: {
  data: UsageData;
  real: RealUsage;
  plan: PlanDef;
  seats: number;
  compact: boolean;
  showSpend: boolean;
  account?: Account;
  envLabel: string;
  envId: string;
  envCount: number;
  onOpenSettings: () => void;
  onRefresh: () => void;
}) {
  const [tab, setTab] = useState<Tab>("overview");
  const [pinned, setPinned] = useState(true);
  const [now, setNow] = useState(Date.now());
  const winRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
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

  // ── Local-log estimates ──
  const localSessionUsed = clamp01(data.session.prompts / plan.sessionBudget);
  const weeklyUsed = clamp01(data.weekly.prompts / plan.weeklyBudget);
  const opusUsed = clamp01(data.weekly.opusPrompts / plan.weeklyOpusBudget);

  const sessionEndsAt = data.session.startedAt + 5 * 3600 * 1000;
  const resetMs = sessionEndsAt - now;

  // ── Authoritative limits from Anthropic when available ──
  const live = real.found && !!real.session;
  const sessionUsed = live ? real.session!.utilization : localSessionUsed;
  const resetIn = live
    ? fmtResetRelative(real.session!.resetsAt, now) || fmtDuration(resetMs)
    : fmtDuration(resetMs);

  const weeklyRows: WeeklyRow[] =
    real.found && real.weekly.length
      ? real.weekly.map((b) => ({
          label: b.label,
          util: b.utilization,
          foot: b.resetsAt ? `resets ${fmtResetAbsolute(b.resetsAt)}` : "",
        }))
      : [
          { label: "Weekly · all models", util: weeklyUsed, foot: "trailing 7 days" },
          { label: "Weekly · Opus only", util: opusUsed, foot: "trailing 7 days" },
        ];

  const last4 = data.burn.slice(-4).reduce((a, b) => a + b, 0); // last hour
  const promptsLeft = Math.max(0, plan.sessionBudget - data.session.prompts);
  const burnPerMin = last4 / 60;

  // Forecast: prefer a live projection off the real session utilization.
  let forecastMin: number;
  let willBust: boolean;
  if (live && real.session!.resetsAt) {
    const resetT = Date.parse(real.session!.resetsAt);
    const sessionStart = resetT - 5 * 3600 * 1000;
    const elapsed = Math.max(60_000, now - sessionStart);
    const u = real.session!.utilization;
    if (u >= 1) {
      forecastMin = 0;
      willBust = true;
    } else if (u <= 0) {
      forecastMin = 999;
      willBust = false;
    } else {
      const msToFull = (elapsed * (1 - u)) / u;
      forecastMin = Math.round(msToFull / 60000);
      willBust = msToFull < resetT - now;
    }
  } else {
    forecastMin = burnPerMin > 0 ? Math.round(promptsLeft / burnPerMin) : 999;
    willBust = forecastMin < resetMs / 60000;
  }

  // Economics: override the estimate with authoritative extra-usage when present.
  let econ = computeEconomics(plan, seats, data.month, now);
  if (real.extra) {
    const extraUsage = real.extra.usedCredits;
    const d = new Date(now);
    const daysInMonth = new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
    const elapsedDays = Math.max(0.5, (now - data.month.startedAt) / 86_400_000);
    const projectedExtra = (extraUsage / elapsedDays) * daysInMonth;
    econ = {
      ...econ,
      extraUsage,
      total: econ.base + extraUsage,
      projectedExtra,
      projectedTotal: econ.base + projectedExtra,
    };
  }
  const syncedAgo = Math.max(0, Math.floor((now - data.generatedAt) / 1000));

  const togglePin = async () => {
    const next = !pinned;
    setPinned(next);
    await win((w) => w.setAlwaysOnTop(next));
  };

  // Hide the Spend tab when expenditures are off; redirect if it was active.
  const visibleTabs = TABS.filter((t) => t !== "spend" || showSpend);
  const activeTab: Tab = tab === "spend" && !showSpend ? "overview" : tab;

  return (
    <div ref={winRef} className={`win ${compact ? "compact" : ""}`}>
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
              {live && <span className="live-badge"><span className="live-dot" />live</span>}
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
          <button className={`chrome-btn pin ${pinned ? "on" : ""}`} onClick={togglePin} title="Always on top">
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

      {/* Tabs */}
      <div className="tabs">
        {visibleTabs.map((id) => (
          <button key={id} className={`tab ${activeTab === id ? "on" : ""}`} onClick={() => setTab(id)}>
            {id}
          </button>
        ))}
        <div className="tab-spacer" />
        <button className="tab icon-only" title="Settings" onClick={onOpenSettings}>
          <Icon name="gear" size={13} />
        </button>
        <button className="tab icon-only" title="Refresh" onClick={onRefresh}>
          <Icon name="refresh" size={13} />
        </button>
      </div>

      <div className="body">
        {activeTab === "overview" && (
          <OverviewTab
            sessionUsed={sessionUsed}
            resetIn={resetIn}
            live={live}
            weeklyRows={weeklyRows}
            forecastLabel={fmtForecast(forecastMin)}
            willBust={willBust}
            promptsLeft={promptsLeft}
            models={data.models}
            burn={data.burn}
            econ={econ}
            showSpend={showSpend}
          />
        )}
        {activeTab === "models" && <ModelsTab models={data.models} />}
        {activeTab === "surfaces" && <SurfacesTab surfaces={data.surfaces} daily={data.daily} />}
        {activeTab === "spend" && <SpendTab plan={plan} econ={econ} month={data.month} extra={real.extra} />}
        {activeTab === "history" && <HistoryTab recent={data.recent} heatmap={data.heatmap} />}
      </div>

      <div className="footer">
        <div className="footer-stat">
          <span className="footer-num mono">{fmtTokens(data.today.tokens)}</span>
          <span className="footer-lbl">tokens today</span>
        </div>
        <div className="footer-divider" />
        <div className="footer-stat">
          <span className="footer-num mono">${data.today.cost.toFixed(2)}</span>
          <span className="footer-lbl">est. cost</span>
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
