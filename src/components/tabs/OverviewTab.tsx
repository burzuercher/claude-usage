import { useCountUp } from "../../hooks/useCountUp";
import { Ring } from "../Ring";
import { StackedBar } from "../StackedBar";
import { BurnSpark } from "../BurnSpark";
import { Icon } from "../Icon";
import { MODEL_COLOR, type ModelStat } from "../../lib/types";
import type { Economics } from "../../lib/plans";

export interface WeeklyRow {
  label: string;
  util: number; // 0..1
  foot: string; // right-aligned note (reset time / "trailing 7 days")
}

export interface OverviewProps {
  sessionUsed: number; // 0..1
  resetIn: string;
  live: boolean; // real API data vs local estimate
  weeklyRows: WeeklyRow[];
  forecastLabel: string;
  willBust: boolean;
  promptsLeft: number;
  models: ModelStat[];
  burn: number[];
  econ: Economics;
  showSpend: boolean;
}

const usd = (n: number) => `$${n.toFixed(2)}`;

function WeeklyBars({ rows, ink }: { rows: WeeklyRow[]; ink: boolean }) {
  return (
    <>
      {rows.map((r, i) => (
        <AnimatedWeekly key={r.label} row={r} ink={ink && i > 0} />
      ))}
    </>
  );
}

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
        <span />
        <span className="mono">{row.foot}</span>
      </div>
    </div>
  );
}

export function OverviewTab(p: OverviewProps) {
  const totalTokens = Math.max(1, p.models.reduce((a, m) => a + m.tokens, 0));
  const segments = p.models.map((m) => ({
    id: m.id,
    share: m.tokens / totalTokens,
    color: MODEL_COLOR[m.id] ?? "var(--moss)",
  }));

  return (
    <>
      <Ring value={p.sessionUsed} resetIn={p.resetIn} />

      {/* Forecast pill (burn-based, from local logs) */}
      <div className={`forecast ${p.willBust ? "warn" : ""}`}>
        <Icon name="spark" size={13} />
        <span>
          At this pace, you'll <b>{p.willBust ? "hit the cap" : "finish under cap"}</b> in{" "}
          <b className="mono">{p.forecastLabel}</b>
        </span>
      </div>

      {/* Weekly limits — real categories when live, else estimated */}
      <div className="weekly">
        <WeeklyBars rows={p.weeklyRows} ink />
        <div className="weekly-src">
          {p.live ? (
            <><span className="live-dot" /> live from Claude · limits</>
          ) : (
            <>estimated from local logs · set plan in settings</>
          )}
        </div>
      </div>

      {/* Expenditure mini card (hidden unless enabled in settings) */}
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
              <span className="spend-mini-lbl">extra usage</span>
            </div>
          </div>
        </div>
      )}

      {/* Model split mini (from local logs) */}
      <div className="block">
        <div className="block-head">
          <span className="block-title">This session · models</span>
          <span className="block-meta mono">{p.promptsLeft} left</span>
        </div>
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
      </div>

      {/* Burn rate spark (from local logs) */}
      <div className="block">
        <div className="block-head">
          <span className="block-title">Burn rate · last 5h</span>
          <span className="block-meta">
            <span className="dot" style={{ background: "var(--clay)" }} /> now
            <span className="dot" style={{ background: "transparent", border: "1px dashed var(--ink-40)" }} /> safe
          </span>
        </div>
        <BurnSpark data={p.burn} threshold={Math.max(...p.burn, 1) * 0.72} />
      </div>
    </>
  );
}
