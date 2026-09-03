import { useEffect, useState } from "react";
import type { Attribution, SessionStat } from "../lib/types";
import { fmtAgo, fmtDuration, fmtTokens } from "../lib/format";

/** Strip the vendor prefix and date suffix so a model id fits the detail line. */
function shortModel(id: string): string {
  return id.replace(/^claude-/, "").replace(/-\d{8}$/, "");
}

export function Sessions({
  attr,
  loading,
  now,
}: {
  attr: Attribution;
  loading: boolean;
  now: number;
}) {
  const [selectedId, setSelectedId] = useState("");

  // Follow the newest session until the user picks one, and fall back to it if
  // the chosen session ages out of the 7-day window.
  const selected: SessionStat | undefined =
    attr.sessions.find((s) => s.id === selectedId) ?? attr.sessions[0];
  useEffect(() => {
    if (selectedId && !attr.sessions.some((s) => s.id === selectedId)) setSelectedId("");
  }, [attr.sessions, selectedId]);

  if (loading && !attr.found) {
    return (
      <div className="empty-hint loading">
        <span className="spinner" />
        Scanning local sessions…
      </div>
    );
  }
  if (!selected) {
    return (
      <div className="empty-hint">
        No local session transcripts from the last 7 days.
        <span className="mono">this account logs nothing on this machine</span>
      </div>
    );
  }

  // Wall time: Claude Code's own figure when it has written one, else the span
  // the transcript actually covers.
  const wall = selected.hasCostState ? selected.wallMs : selected.lastAt - selected.startedAt;
  const totalTokens = selected.input + selected.output + selected.cacheRead + selected.cacheWrite;

  return (
    <>
      <div className="block">
        <div className="block-head">
          <span className="block-title">Sessions · last 7 days</span>
          <span className="block-meta mono">{attr.sessions.length}</span>
        </div>
        <div className="sess-list">
          {attr.sessions.map((s) => (
            <button
              key={s.id}
              className={`sess-item ${s.id === selected.id ? "on" : ""}`}
              onClick={() => setSelectedId(s.id)}
              title={`${s.label}\n${s.project}${s.branch ? ` · ${s.branch}` : ""}\n${s.id}`}
            >
              <span className="sess-item-main">
                <span className="sess-item-label">{s.label}</span>
                <span className="sess-item-proj">{s.project}</span>
              </span>
              <span className="sess-item-ago mono">{fmtAgo(s.lastAt, now) || "just now"}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="block">
        <div className="block-head">
          <span className="block-title sess-title">{selected.label}</span>
          {selected.branch && <span className="block-meta">{selected.branch}</span>}
        </div>
        <div className="spend-mini-row">
          <div className="spend-mini-cell">
            <span className="spend-mini-num mono">
              {selected.hasCostState ? `$${selected.cost.toFixed(2)}` : "—"}
            </span>
            <span className="spend-mini-lbl">cost</span>
          </div>
          <div className="spend-mini-cell">
            <span className="spend-mini-num mono">{selected.turns}</span>
            <span className="spend-mini-lbl">turns</span>
          </div>
          <div className="spend-mini-cell">
            <span className="spend-mini-num mono">{fmtDuration(wall)}</span>
            <span className="spend-mini-lbl">wall</span>
          </div>
        </div>

        <div className="sess-detail">
          {selected.hasCostState ? (
            <div>
              api <b className="mono">{fmtDuration(selected.apiMs)}</b> · tools{" "}
              <b className="mono">{fmtDuration(selected.toolMs)}</b> ·{" "}
              <b className="mono">
                +{selected.linesAdded}/-{selected.linesRemoved}
              </b>{" "}
              lines
            </div>
          ) : (
            <div className="sess-pending">
              Cost and durations appear once Claude Code has written its first cost record for
              this session.
            </div>
          )}
          <div>
            in <b className="mono">{fmtTokens(selected.input)}</b> · out{" "}
            <b className="mono">{fmtTokens(selected.output)}</b> · cache r{" "}
            <b className="mono">{fmtTokens(selected.cacheRead)}</b> · w{" "}
            <b className="mono">{fmtTokens(selected.cacheWrite)}</b> ={" "}
            <b className="mono">{fmtTokens(totalTokens)}</b>
          </div>
          {selected.models.length > 0 && <div>{selected.models.map(shortModel).join(" · ")}</div>}
        </div>
      </div>
    </>
  );
}
