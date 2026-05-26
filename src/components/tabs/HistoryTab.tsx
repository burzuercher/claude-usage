import { fmtTokens } from "../../lib/format";
import { MODEL_COLOR, type RecentTask, type HeatRow } from "../../lib/types";

export function HistoryTab({ recent, heatmap }: { recent: RecentTask[]; heatmap: HeatRow[] }) {
  return (
    <div className="history">
      <div className="block-head">
        <span className="block-title">Recent tasks</span>
        <span className="block-meta">latest sessions</span>
      </div>
      <div className="hist-list">
        {recent.map((r, i) => (
          <div className="hist-row" key={i}>
            <span className="hist-dot" style={{ background: MODEL_COLOR[r.model] ?? "var(--moss)" }} />
            <div className="hist-body">
              <div className="hist-task">{r.task}</div>
              <div className="hist-meta mono">{fmtTokens(r.tokens)} · {r.t}</div>
            </div>
            <div className="hist-spark"><MiniBars seed={r.ts} /></div>
          </div>
        ))}
        {recent.length === 0 && <div className="empty-hint">No recent sessions found.</div>}
      </div>

      <div className="block">
        <div className="block-head">
          <span className="block-title">Heat · hours of day</span>
        </div>
        <Heatmap rows={heatmap} />
      </div>
    </div>
  );
}

// Deterministic mini sparkline so it doesn't flicker on every poll.
function MiniBars({ seed }: { seed: number }) {
  const vals = Array.from({ length: 8 }, (_, i) => {
    const x = Math.sin(seed * 0.0001 + i * 1.7) * 0.5 + 0.5;
    return 0.3 + x * 0.7;
  });
  return (
    <div className="mini-bars">
      {vals.map((v, i) => <span key={i} style={{ height: `${v * 100}%` }} />)}
    </div>
  );
}

function Heatmap({ rows }: { rows: HeatRow[] }) {
  return (
    <div className="heat">
      {rows.map((row, ri) => (
        <div className="heat-row" key={ri}>
          <span className="heat-day">{row.label}</span>
          {row.cells.map((v, ci) => (
            <span key={ci} className="heat-cell" style={{ background: `rgba(217, 119, 87, ${v.toFixed(2)})` }} />
          ))}
        </div>
      ))}
    </div>
  );
}
