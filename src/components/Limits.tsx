import { useState } from "react";
import type { AttrWindow, Attribution, Share } from "../lib/types";

/** Behavior copy, kept verbatim from Claude Code's own /usage screen so the two
 *  read the same. `headline` takes the percentage. */
const BEHAVIOR: Record<string, { headline: (pct: number) => string; body: string }> = {
  cache_miss: {
    headline: (p) => `${p}% of your usage hit a >100k-token cache miss`,
    body: "Uncached input is expensive, and often happens when sending a message to a session that has gone idle. /compact before stepping away keeps the cold-start small.",
  },
  long_context: {
    headline: (p) => `${p}% of your usage was at >150k context`,
    body: "Longer sessions are more expensive even when cached. /compact mid-task, /clear when switching to new tasks.",
  },
  subagent_heavy: {
    headline: (p) => `${p}% of your usage came from subagent-heavy sessions`,
    body: "Each subagent runs its own requests. Be deliberate about spawning them — and consider configuring a cheaper model for simpler subagents.",
  },
  high_parallel: {
    headline: (p) => `${p}% of your usage was while 4+ sessions ran in parallel`,
    body: "All sessions share one limit. If you don't need them all at once, queueing uses it more evenly.",
  },
  cron: {
    headline: (p) => `${p}% of your usage came from sessions active for 8+ hours`,
    body: "These are often background/loop sessions. Continuous usage can add up quickly so make sure it is intentional.",
  },
};

/** What a behavior's `count` counts — the backend tallies requests for some and
 *  whole sessions for others, so it can only be labelled per key. */
const COUNTS_SESSIONS = new Set(["subagent_heavy", "cron"]);

/** Rows shown per share table before collapsing into "… N more" — Claude Code's own cap. */
const MAX_ROWS = 8;
/** Behaviors under this share aren't reported; quoted in the empty state. */
const MIN_PCT = 10;

export function Limits({ attr, loading }: { attr: Attribution; loading: boolean }) {
  const [span, setSpan] = useState<"day" | "week">("day");
  const w: AttrWindow = span === "day" ? attr.day : attr.week;
  const spanLabel = span === "day" ? "24h" : "7d";

  const tables: { title: string; rows: Share[]; prefix?: string }[] = [
    { title: "Skills", rows: w.skills, prefix: "/" },
    { title: "Subagents", rows: w.agents },
    { title: "MCP servers", rows: w.mcpServers },
    { title: "Plugins", rows: w.plugins },
  ];
  const hasTables = tables.some((t) => t.rows.length > 0);

  return (
    <>
      <div className="block">
        <div className="block-head">
          <span className="block-title">What&rsquo;s contributing</span>
          <div className="sp-seg span-seg">
            <button className={span === "day" ? "on" : ""} onClick={() => setSpan("day")}>
              24h
            </button>
            <button className={span === "week" ? "on" : ""} onClick={() => setSpan("week")}>
              7d
            </button>
          </div>
        </div>
        <div className="attr-note">
          Approximate, based on local sessions on this machine — does not include other devices or claude.ai
        </div>
      </div>

      {loading && !attr.found ? (
        <div className="empty-hint loading">
          <span className="spinner" />
          Scanning local sessions…
        </div>
      ) : !attr.found ? (
        <div className="empty-hint">
          No local session transcripts from the last 7 days.
          <span className="mono">nothing to attribute yet</span>
        </div>
      ) : (
        <>
          <div className="attr-sub">
            Last {spanLabel} · these are independent characteristics of your usage, not a breakdown
          </div>

          {w.behaviors.length === 0 && !hasTables ? (
            <div className="empty-hint">Nothing over {MIN_PCT}% in this period — try the other window.</div>
          ) : (
            <>
              {w.behaviors.map((b) => {
                const copy = BEHAVIOR[b.key];
                if (!copy) return null;
                const unit = COUNTS_SESSIONS.has(b.key) ? "session" : "request";
                return (
                  <div
                    className="behavior"
                    key={b.key}
                    title={`${b.count} ${unit}${b.count === 1 ? "" : "s"} in the last ${spanLabel}`}
                  >
                    <div className="behavior-head">{copy.headline(b.pct)}</div>
                    <div className="bar slim">
                      <div className="bar-fill clay" style={{ width: `${Math.min(100, b.pct)}%` }} />
                    </div>
                    <div className="behavior-body">{copy.body}</div>
                  </div>
                );
              })}

              {tables.map((t) => (
                <ShareTable key={t.title} title={t.title} rows={t.rows} prefix={t.prefix} />
              ))}
            </>
          )}
        </>
      )}
    </>
  );
}

function ShareTable({ title, rows, prefix = "" }: { title: string; rows: Share[]; prefix?: string }) {
  if (rows.length === 0) return null;
  const shown = rows.slice(0, MAX_ROWS);
  const more = rows.length - shown.length;
  return (
    <div className="block">
      <div className="block-head">
        <span className="block-title">{title}</span>
        <span className="block-meta">% of usage</span>
      </div>
      <div className="share-rows">
        {shown.map((r) => (
          <div className="share-row" key={r.name}>
            <span className="share-name" title={`${prefix}${r.name}`}>
              {prefix}
              {r.name}
            </span>
            <span className="share-val mono">{r.pct}%</span>
          </div>
        ))}
        {more > 0 && <div className="share-more">… {more} more</div>}
      </div>
    </div>
  );
}
