import { Icon, type IconName } from "../Icon";
import type { SurfaceStat, DailyStat } from "../../lib/types";

const SURFACE_ICON: Record<string, IconName> = {
  code: "code",
  chat: "chat",
  other: "cowork",
};

export function SurfacesTab({ surfaces, daily }: { surfaces: SurfaceStat[]; daily: DailyStat[] }) {
  const total = Math.max(1, surfaces.reduce((a, s) => a + s.prompts, 0));
  const top = [...surfaces].sort((a, b) => b.prompts - a.prompts)[0];

  return (
    <div className="surfaces">
      <div className="surface-note">
        {top ? `${top.name} is consuming most of your local usage. ` : ""}
        claude.ai web chat isn't logged locally, so it's not shown.
      </div>
      {surfaces.map((s) => {
        const share = s.prompts / total;
        return (
          <div className="surface-row" key={s.id}>
            <div className="surface-icon"><Icon name={SURFACE_ICON[s.id] ?? "cowork"} size={16} /></div>
            <div className="surface-body">
              <div className="surface-head">
                <b>{s.name}</b>
                <span className="mono">{Math.round(share * 100)}%</span>
              </div>
              <div className="bar slim">
                <div className="bar-fill clay" style={{ width: `${share * 100}%` }} />
              </div>
              <div className="surface-meta mono">{s.prompts} prompts · 7 days</div>
            </div>
          </div>
        );
      })}
      {surfaces.length === 0 && <div className="empty-hint">No surface activity in the last 7 days.</div>}

      <div className="block">
        <div className="block-head">
          <span className="block-title">Daily split · last 7 days</span>
        </div>
        <DailySplit days={daily} />
      </div>
    </div>
  );
}

function DailySplit({ days }: { days: DailyStat[] }) {
  const max = Math.max(1, ...days.map((x) => x.code + x.chat + x.other));
  return (
    <div className="daily">
      {days.map((day, i) => {
        const total = day.code + day.chat + day.other;
        const h = (total / max) * 78;
        return (
          <div className="daily-col" key={i}>
            <div className="daily-bar" style={{ height: `${h}px` }}>
              <div style={{ flex: day.code, background: "var(--ink)" }} />
              <div style={{ flex: day.chat, background: "var(--clay)" }} />
              <div style={{ flex: day.other, background: "var(--sand)" }} />
            </div>
            <div className="daily-lbl">{day.label}</div>
          </div>
        );
      })}
    </div>
  );
}
