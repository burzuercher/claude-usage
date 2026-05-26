import { Icon } from "../Icon";
import { fmtTokens } from "../../lib/format";
import { MODEL_COLOR, type ModelStat } from "../../lib/types";

const NOTES: Record<string, string> = {
  sonnet: "workhorse · drafting, code review",
  opus: "reasoning · long-context · weekly limited",
  haiku: "fast · lookups · cheap",
  other: "misc models",
};

export function ModelsTab({ models }: { models: ModelStat[] }) {
  const totalTokens = Math.max(1, models.reduce((a, m) => a + m.tokens, 0));
  const maxShare = Math.max(0.0001, ...models.map((m) => m.tokens / totalTokens));
  const opusHeavy = models.some((m) => m.id === "opus" && m.tokens / totalTokens > 0.3);

  return (
    <div className="models">
      {models.map((m) => {
        const share = m.tokens / totalTokens;
        const color = MODEL_COLOR[m.id] ?? "var(--moss)";
        return (
          <div className="model-card" key={m.id}>
            <div className="model-head">
              <div className="model-name">
                <span className="model-dot" style={{ background: color }} />
                <b>{m.name}</b>
              </div>
              <span className="model-pct mono">{Math.round(share * 100)}%</span>
            </div>
            <div className="bar">
              <div className="bar-fill" style={{ width: `${(share / maxShare) * 100}%`, background: color }} />
            </div>
            <div className="model-meta">
              <span className="model-note">{NOTES[m.id] ?? NOTES.other}</span>
              <span className="model-stat">
                <span className="mono">{fmtTokens(m.tokens)}</span>
                <span className="sep">·</span>
                <span className="mono">${m.cost.toFixed(2)}</span>
              </span>
            </div>
          </div>
        );
      })}
      {models.length === 0 && <div className="empty-hint">No model usage in the last 7 days.</div>}
      {opusHeavy && (
        <div className="tip">
          <Icon name="spark" size={12} />
          <span>Opus is a big share of your week — switching routine tasks to Sonnet stretches your budget.</span>
        </div>
      )}
    </div>
  );
}
