//! Local usage *attribution* — the second half of Claude Code's `/usage`
//! screen, which the live usage API does not serve.
//!
//! Two things live here:
//!
//! 1. **"What's contributing to your limits usage?"** — which behaviors drive
//!    spend (long context, cold caches, subagent-heavy sessions, 8h+ sessions,
//!    parallel sessions) and a share breakdown by skill, subagent, plugin and
//!    MCP server, over the last 24 hours and the last 7 days.
//! 2. **Per-session accounting** — Claude Code writes a `cost-state` record
//!    carrying its own authoritative `totalCostUSD`, durations and line counts,
//!    so a session's real cost can be shown without estimating anything.
//!
//! The attribution numbers are deliberately a faithful re-implementation of
//! Claude Code 2.1.259's own algorithm — same fields, same thresholds, same
//! weighting — so the widget agrees with what `/usage` prints. The cost weight
//! is a *synthetic relative unit* used only to compute percentages; it is never
//! dollars and must never be rendered as such.
//!
//! Like the CLI's own screen this is approximate and machine-local: it sees
//! only sessions logged on this machine, not other devices or claude.ai.
//!
//! Not covered: the CLI also attributes `/loop` runs, which needs a separate
//! record type and a fire-batching pass over prompt records.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use walkdir::WalkDir;

const DAY_MS: i64 = 24 * 3600 * 1000;
const WEEK_MS: i64 = 7 * DAY_MS;

// ─── Claude Code's thresholds (2.1.259) ────────────────────────────────────

/// Uncached input above this on a single turn is a cold-cache hit.
const CACHE_MISS_TOKENS: u64 = 100_000;
/// Prompt size (cache read + cache write + uncached) above this is "long context".
const LONG_CTX_TOKENS: u64 = 150_000;
/// Distinct sessions inside one bucket that count as running "in parallel".
const PARALLEL_SESSIONS: usize = 4;
/// Subagent turns in a session that make it "subagent-heavy"...
const SUBAGENT_TURNS: u32 = 3;
/// ...or this share of the session's cost coming from subagents.
const SUBAGENT_COST_SHARE: f64 = 0.5;
/// Distinct wall-clock hours a session touches to count as long-running.
const LONG_SESSION_HOURS: usize = 8;
/// Bucket width for the parallel-sessions test.
const BUCKET_MS: i64 = 5 * 60 * 1000;
/// Behaviors below this share of total usage are not worth reporting.
const MIN_BEHAVIOR_PCT: f64 = 10.0;

// ─── Output shapes (camelCase for the frontend) ────────────────────────────

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    /// False when the logs root is missing or nothing landed in the window.
    pub found: bool,
    pub generated_at: i64,
    /// Transcripts actually read (mtime within the 7-day window).
    pub scanned_files: u32,

    /// Last 24 hours.
    pub day: AttrWindow,
    /// Last 7 days.
    pub week: AttrWindow,

    /// Sessions with at least one turn in the last 7 days, newest first.
    pub sessions: Vec<SessionStat>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AttrWindow {
    pub request_count: u32,
    pub session_count: u32,
    /// Independent characteristics of the usage, not a partition — the
    /// percentages overlap and do not sum to 100.
    pub behaviors: Vec<Behavior>,
    /// Subagents, keyed by the skill running inside them when there is one.
    pub agents: Vec<Share>,
    pub skills: Vec<Share>,
    pub plugins: Vec<Share>,
    pub mcp_servers: Vec<Share>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Behavior {
    /// "cache_miss" | "long_context" | "subagent_heavy" | "high_parallel" | "cron"
    pub key: String,
    pub pct: u32,
    /// Requests for cache_miss/long_context/high_parallel; sessions for the rest.
    pub count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub name: String,
    pub pct: u32,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionStat {
    pub id: String,
    /// Claude Code's generated title, else the prompt slug, else a short id.
    pub label: String,
    pub project: String,
    pub branch: String,
    pub started_at: i64,
    pub last_at: i64,
    pub turns: u32,

    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Raw model ids seen, e.g. "claude-opus-5[1m]".
    pub models: Vec<String>,

    /// True once Claude Code has written a `cost-state` record for the session.
    /// While a session is still live there is none, so `cost` and the durations
    /// below are not yet knowable — the frontend shows "—" rather than an
    /// estimate.
    pub has_cost_state: bool,
    pub cost: f64,
    pub api_ms: i64,
    pub wall_ms: i64,
    pub tool_ms: i64,
    pub lines_added: u32,
    pub lines_removed: u32,
}

// ─── Internal per-turn record ──────────────────────────────────────────────

struct Rec {
    ts_ms: i64,
    session_id: String,
    /// `input_tokens` — uncached prompt input.
    uncached: u64,
    output: u64,
    cache_create: u64,
    /// `cache_read_input_tokens`.
    cached: u64,
    is_subagent: bool,
    model_tier: f64,
    attribution_agent: Option<String>,
    attribution_skill: Option<String>,
    attribution_plugin: Option<String>,
    attribution_mcp_server: Option<String>,
}

/// Relative price weight of a turn. Mirrors Claude Code exactly: cache reads
/// count 1, uncached input 10, cache writes 12.5, output 50, all scaled by the
/// model's tier. A synthetic unit for percentages only — never dollars.
fn weight(r: &Rec) -> f64 {
    (r.cached as f64
        + r.uncached as f64 * 10.0
        + r.cache_create as f64 * 12.5
        + r.output as f64 * 50.0)
        * r.model_tier
}

/// Relative cost multiplier per model family, as Claude Code weights them.
fn model_tier(model: &str) -> f64 {
    if model.is_empty() {
        return 3.0;
    }
    let m = model.to_ascii_lowercase();
    if m.contains("fable") {
        10.0
    } else if m.contains("opus") {
        5.0
    } else if m.contains("haiku") {
        1.0
    } else {
        3.0
    }
}

// ─── Accumulators ──────────────────────────────────────────────────────────

#[derive(Default)]
struct SessAcc {
    cost: f64,
    sub_cost: f64,
    sub_count: u32,
    hours: HashSet<i64>,
}

#[derive(Default)]
struct BucketAcc {
    sids: HashSet<String>,
    cost: f64,
    count: u32,
}

#[derive(Default)]
struct Acc {
    total: f64,
    requests: u32,
    cache_miss_cost: f64,
    cache_miss_count: u32,
    long_ctx_cost: f64,
    long_ctx_count: u32,
    sessions: HashMap<String, SessAcc>,
    buckets: HashMap<i64, BucketAcc>,
    by_agent: HashMap<String, f64>,
    by_skill: HashMap<String, f64>,
    by_plugin: HashMap<String, f64>,
    by_mcp_server: HashMap<String, f64>,
}

fn bump(map: &mut HashMap<String, f64>, key: Option<&String>, w: f64) {
    if let Some(k) = key {
        if !k.is_empty() {
            *map.entry(k.clone()).or_insert(0.0) += w;
        }
    }
}

impl Acc {
    fn fold(&mut self, r: &Rec) {
        let w = weight(r);
        self.total += w;
        self.requests += 1;

        // A skill running inside a subagent is attributed to the subagent, not
        // to the skill list — the two are alternatives, never both.
        if r.attribution_agent.is_some() {
            let key = r.attribution_skill.as_ref().or(r.attribution_agent.as_ref());
            bump(&mut self.by_agent, key, w);
        } else {
            bump(&mut self.by_skill, r.attribution_skill.as_ref(), w);
        }
        bump(&mut self.by_plugin, r.attribution_plugin.as_ref(), w);
        bump(&mut self.by_mcp_server, r.attribution_mcp_server.as_ref(), w);

        let prompt = r.cached + r.cache_create + r.uncached;
        if r.uncached > CACHE_MISS_TOKENS {
            self.cache_miss_cost += w;
            self.cache_miss_count += 1;
        }
        if prompt > LONG_CTX_TOKENS {
            self.long_ctx_cost += w;
            self.long_ctx_count += 1;
        }

        let s = self.sessions.entry(r.session_id.clone()).or_default();
        s.cost += w;
        if r.is_subagent {
            s.sub_cost += w;
            s.sub_count += 1;
        }
        s.hours.insert(r.ts_ms / 3_600_000);

        let b = self.buckets.entry(r.ts_ms / BUCKET_MS).or_default();
        b.sids.insert(r.session_id.clone());
        b.cost += w;
        b.count += 1;
    }

    fn finish(self) -> AttrWindow {
        let total = self.total;

        // 4+ sessions active inside the same 5-minute bucket.
        let mut parallel_cost = 0.0;
        let mut parallel_count = 0u32;
        for b in self.buckets.values() {
            if b.sids.len() >= PARALLEL_SESSIONS {
                parallel_cost += b.cost;
                parallel_count += b.count;
            }
        }

        let mut subagent_cost = 0.0;
        let mut subagent_sessions = 0u32;
        let mut long_cost = 0.0;
        let mut long_sessions = 0u32;
        for s in self.sessions.values() {
            if s.sub_count >= SUBAGENT_TURNS
                || (s.cost > 0.0 && s.sub_cost / s.cost > SUBAGENT_COST_SHARE)
            {
                subagent_cost += s.cost;
                subagent_sessions += 1;
            }
            if s.hours.len() >= LONG_SESSION_HOURS {
                long_cost += s.cost;
                long_sessions += 1;
            }
        }

        let mut raw = vec![
            ("cache_miss", self.cache_miss_cost, self.cache_miss_count),
            ("long_context", self.long_ctx_cost, self.long_ctx_count),
            ("subagent_heavy", subagent_cost, subagent_sessions),
            ("high_parallel", parallel_cost, parallel_count),
            ("cron", long_cost, long_sessions),
        ];
        raw.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let behaviors = raw
            .into_iter()
            .filter(|(_, cost, _)| total > 0.0 && cost / total * 100.0 >= MIN_BEHAVIOR_PCT)
            .map(|(key, cost, count)| Behavior {
                key: key.to_string(),
                pct: (cost / total * 100.0).round() as u32,
                count,
            })
            .collect();

        AttrWindow {
            request_count: self.requests,
            session_count: self.sessions.len() as u32,
            behaviors,
            agents: shares(self.by_agent, total),
            skills: shares(self.by_skill, total),
            plugins: shares(self.by_plugin, total),
            mcp_servers: shares(self.by_mcp_server, total),
        }
    }
}

/// A name → cost map turned into descending percentage shares, dropping
/// anything that rounds to 0%.
fn shares(map: HashMap<String, f64>, total: f64) -> Vec<Share> {
    if map.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let mut v: Vec<(String, f64)> = map.into_iter().collect();
    // Cost desc, then name — a stable order between equal shares.
    v.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    v.into_iter()
        .map(|(name, cost)| Share {
            name,
            pct: (cost / total * 100.0).round() as u32,
        })
        .filter(|s| s.pct > 0)
        .collect()
}

// ─── Session metadata (collected in the same pass) ─────────────────────────

#[derive(Default)]
struct SessMeta {
    title: String,
    slug: String,
    cwd: String,
    branch: String,
    project: String,
    first_ts: i64,
    last_ts: i64,
    turns: u32,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    models: Vec<String>,
    has_cost_state: bool,
    cost: f64,
    start_time: i64,
    api_ms: i64,
    wall_ms: i64,
    tool_ms: i64,
    lines_added: u32,
    lines_removed: u32,
}

fn push_model(models: &mut Vec<String>, name: &str) {
    if !name.is_empty() && !models.iter().any(|m| m == name) {
        models.push(name.to_string());
    }
}

// ─── Entry point ───────────────────────────────────────────────────────────

/// Walk a `projects/` directory and build the attribution view.
///
/// Only transcripts touched within the last 7 days are read: the window cannot
/// contain anything older, and on a real tree that mtime filter cuts the scan
/// by roughly 6×.
pub fn collect_at(root: Option<std::path::PathBuf>) -> Attribution {
    let now_ms = Utc::now().timestamp_millis();
    let root = match root {
        Some(r) if r.exists() => r,
        _ => {
            return Attribution {
                generated_at: now_ms,
                ..Default::default()
            }
        }
    };

    let week_cutoff = now_ms - WEEK_MS;
    let day_cutoff = now_ms - DAY_MS;

    let mut week = Acc::default();
    let mut day = Acc::default();
    let mut metas: HashMap<String, SessMeta> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut scanned_files = 0u32;

    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
    {
        // Skip transcripts too old to hold anything inside the window.
        let mtime_ms = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);
        match mtime_ms {
            Some(ms) if ms >= week_cutoff => {}
            _ => continue,
        }

        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        scanned_files += 1;

        // Directory name under `projects/`, the fallback when a record has no cwd.
        let project_dir = entry
            .path()
            .strip_prefix(&root)
            .ok()
            .and_then(|p| p.components().next())
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_default();

        for line in content.lines() {
            if line.is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let kind = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
            let sid = match v
                .get("sessionId")
                .or_else(|| v.get("session_id"))
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
            {
                Some(s) => s,
                None => continue,
            };

            match kind {
                // Claude Code's own accounting. Rewritten as the session runs,
                // so the last record we see wins.
                "cost-state" => {
                    let g = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
                    let m = metas.entry(sid.to_string()).or_default();
                    m.has_cost_state = true;
                    m.cost = v.get("totalCostUSD").and_then(|x| x.as_f64()).unwrap_or(0.0);
                    m.api_ms = g("totalAPIDuration");
                    m.wall_ms = g("totalDuration");
                    m.tool_ms = g("totalToolDuration");
                    m.lines_added = g("totalLinesAdded").max(0) as u32;
                    m.lines_removed = g("totalLinesRemoved").max(0) as u32;
                    m.start_time = g("startTime");
                    if let Some(mu) = v.get("modelUsage").and_then(|x| x.as_object()) {
                        for name in mu.keys() {
                            push_model(&mut m.models, name);
                        }
                    }
                    continue;
                }
                // Claude Code's generated session title.
                "ai-title" => {
                    if let Some(t) = v
                        .get("aiTitle")
                        .and_then(|x| x.as_str())
                        .filter(|s| !s.is_empty())
                    {
                        metas.entry(sid.to_string()).or_default().title = t.to_string();
                    }
                    continue;
                }
                _ => {}
            }

            // Any record type can carry the session's identifying context.
            {
                let m = metas.entry(sid.to_string()).or_default();
                if m.project.is_empty() {
                    m.project = project_dir.clone();
                }
                for (field, slot) in [
                    ("slug", &mut m.slug),
                    ("cwd", &mut m.cwd),
                    ("gitBranch", &mut m.branch),
                ] {
                    if slot.is_empty() {
                        if let Some(s) = v
                            .get(field)
                            .and_then(|x| x.as_str())
                            .filter(|s| !s.is_empty())
                        {
                            *slot = s.to_string();
                        }
                    }
                }
            }

            if kind != "assistant" {
                continue;
            }
            let msg = match v.get("message") {
                Some(m) => m,
                None => continue,
            };
            let usage = match msg.get("usage") {
                Some(u) => u,
                None => continue,
            };
            let ts_ms = match v.get("timestamp").and_then(|x| x.as_str()) {
                Some(s) => match DateTime::parse_from_rfc3339(s) {
                    Ok(t) => t.with_timezone(&Utc).timestamp_millis(),
                    Err(_) => continue,
                },
                None => continue,
            };
            if ts_ms < week_cutoff {
                continue;
            }

            let g = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
            let uncached = g("input_tokens");
            let output = g("output_tokens");
            let cache_create = g("cache_creation_input_tokens");
            let cached = g("cache_read_input_tokens");
            if uncached + output + cache_create + cached == 0 {
                continue;
            }

            // Dedup retried/resumed turns. Claude Code keys on requestId first,
            // falling back to the message id and then the record uuid.
            let str_of = |v: &serde_json::Value, k: &str| {
                v.get(k)
                    .and_then(|x| x.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            };
            let key = str_of(&v, "requestId")
                .or_else(|| str_of(msg, "id"))
                .or_else(|| str_of(&v, "uuid"))
                .unwrap_or_default();
            if !key.is_empty() && !seen.insert(key) {
                continue;
            }

            let model = msg.get("model").and_then(|x| x.as_str()).unwrap_or("");
            let rec = Rec {
                ts_ms,
                session_id: sid.to_string(),
                uncached,
                output,
                cache_create,
                cached,
                is_subagent: v
                    .get("isSidechain")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                model_tier: model_tier(model),
                attribution_agent: str_of(&v, "attributionAgent"),
                attribution_skill: str_of(&v, "attributionSkill"),
                attribution_plugin: str_of(&v, "attributionPlugin"),
                attribution_mcp_server: str_of(&v, "attributionMcpServer"),
            };

            week.fold(&rec);
            if ts_ms >= day_cutoff {
                day.fold(&rec);
            }

            let m = metas.entry(sid.to_string()).or_default();
            m.turns += 1;
            m.input += uncached;
            m.output += output;
            m.cache_write += cache_create;
            m.cache_read += cached;
            if m.first_ts == 0 || ts_ms < m.first_ts {
                m.first_ts = ts_ms;
            }
            if ts_ms > m.last_ts {
                m.last_ts = ts_ms;
            }
            push_model(&mut m.models, model);
        }
    }

    if week.requests == 0 {
        return Attribution {
            generated_at: now_ms,
            scanned_files,
            ..Default::default()
        };
    }

    let mut sessions: Vec<SessionStat> = metas
        .into_iter()
        .filter(|(_, m)| m.turns > 0)
        .map(|(id, m)| {
            let label = if !m.title.is_empty() {
                m.title.clone()
            } else if !m.slug.is_empty() {
                m.slug.clone()
            } else {
                id.chars().take(8).collect()
            };
            // Prefer the working directory's own name over the mangled
            // `C--Users-…` project directory name.
            let project = match m.cwd.rsplit(|c| c == '/' || c == '\\').next() {
                Some(base) if !base.is_empty() => base.to_string(),
                _ => m.project.clone(),
            };
            SessionStat {
                id,
                label,
                project,
                branch: m.branch,
                started_at: if m.start_time > 0 {
                    m.start_time
                } else {
                    m.first_ts
                },
                last_at: m.last_ts,
                turns: m.turns,
                input: m.input,
                output: m.output,
                cache_read: m.cache_read,
                cache_write: m.cache_write,
                models: m.models,
                has_cost_state: m.has_cost_state,
                cost: m.cost,
                api_ms: m.api_ms,
                wall_ms: m.wall_ms,
                tool_ms: m.tool_ms,
                lines_added: m.lines_added,
                lines_removed: m.lines_removed,
            }
        })
        .collect();
    sessions.sort_by(|a, b| b.last_at.cmp(&a.last_at));

    Attribution {
        found: true,
        generated_at: now_ms,
        scanned_files,
        day: day.finish(),
        week: week.finish(),
        sessions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::io::Write;

    fn temp_subdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "cu-attr-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// One assistant turn, as a transcript line.
    struct Turn {
        session: &'static str,
        req: &'static str,
        model: &'static str,
        input: u64,
        output: u64,
        cache_w: u64,
        cache_r: u64,
        minutes_ago: i64,
        sidechain: bool,
        extra: String,
    }

    impl Turn {
        fn new(session: &'static str, req: &'static str) -> Self {
            Turn {
                session,
                req,
                model: "claude-sonnet-5",
                input: 10,
                output: 10,
                cache_w: 0,
                cache_r: 0,
                minutes_ago: 1,
                sidechain: false,
                extra: String::new(),
            }
        }
        fn tokens(mut self, input: u64, output: u64, cache_w: u64, cache_r: u64) -> Self {
            self.input = input;
            self.output = output;
            self.cache_w = cache_w;
            self.cache_r = cache_r;
            self
        }
        fn model(mut self, m: &'static str) -> Self {
            self.model = m;
            self
        }
        fn at(mut self, minutes_ago: i64) -> Self {
            self.minutes_ago = minutes_ago;
            self
        }
        fn sidechain(mut self) -> Self {
            self.sidechain = true;
            self
        }
        /// Extra top-level JSON fields, e.g. `"attributionSkill":"run"`.
        fn extra(mut self, json: &str) -> Self {
            self.extra = format!(",{json}");
            self
        }
        fn line(&self) -> String {
            let ts = (Utc::now() - Duration::minutes(self.minutes_ago)).to_rfc3339();
            format!(
                r#"{{"type":"assistant","timestamp":"{}","sessionId":"{}","requestId":"{}","isSidechain":{},"message":{{"id":"m-{}","model":"{}","usage":{{"input_tokens":{},"output_tokens":{},"cache_creation_input_tokens":{},"cache_read_input_tokens":{}}}}}{}}}"#,
                ts,
                self.session,
                self.req,
                self.sidechain,
                self.req,
                self.model,
                self.input,
                self.output,
                self.cache_w,
                self.cache_r,
                self.extra
            )
        }
    }

    /// Write turns (plus any raw extra lines) into one transcript and collect.
    fn run(tag: &str, turns: &[Turn], raw: &[String]) -> Attribution {
        let dir = temp_subdir(tag);
        let proj = dir.join("a-project");
        std::fs::create_dir_all(&proj).unwrap();
        {
            let mut f = std::fs::File::create(proj.join("s.jsonl")).unwrap();
            for t in turns {
                writeln!(f, "{}", t.line()).unwrap();
            }
            for l in raw {
                writeln!(f, "{l}").unwrap();
            }
        }
        let a = collect_at(Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
        a
    }

    fn behavior(w: &AttrWindow, key: &str) -> Option<(u32, u32)> {
        w.behaviors
            .iter()
            .find(|b| b.key == key)
            .map(|b| (b.pct, b.count))
    }

    fn share(list: &[Share], name: &str) -> Option<u32> {
        list.iter().find(|s| s.name == name).map(|s| s.pct)
    }

    #[test]
    fn missing_root_is_not_found() {
        let a = collect_at(Some(std::path::PathBuf::from("/definitely/not/here")));
        assert!(!a.found);
        assert_eq!(a.scanned_files, 0);
        assert!(a.sessions.is_empty());

        // An empty tree is walked but yields nothing.
        let dir = temp_subdir("empty");
        let a = collect_at(Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!a.found);
    }

    #[test]
    fn cold_cache_and_long_context_thresholds() {
        // uncached > 100k → cache_miss, and its prompt is over 150k so it counts
        // as long context too (the behaviors overlap by design).
        let a = run(
            "thresholds",
            &[
                Turn::new("s1", "r1").tokens(120_000, 10, 0, 40_000),
                // Prompt of 160k but only 10 uncached: long context only.
                Turn::new("s1", "r2").tokens(10, 10, 0, 160_000),
            ],
            &[],
        );
        assert!(a.found);
        assert_eq!(a.week.request_count, 2);

        let (miss_pct, miss_count) = behavior(&a.week, "cache_miss").expect("cache_miss");
        assert_eq!(miss_count, 1);
        let (ctx_pct, ctx_count) = behavior(&a.week, "long_context").expect("long_context");
        assert_eq!(ctx_count, 2);
        // Uncached input weighs 10x a cache read, so the first turn dominates.
        assert!(miss_pct > 50, "cache_miss pct {miss_pct}");
        assert_eq!(ctx_pct, 100);
    }

    #[test]
    fn boundaries_are_exclusive() {
        // Exactly at a threshold is not over it.
        let a = run(
            "exact",
            &[Turn::new("s1", "r1").tokens(100_000, 0, 50_000, 0)],
            &[],
        );
        assert!(a.found);
        assert_eq!(behavior(&a.week, "cache_miss"), None);
        assert_eq!(behavior(&a.week, "long_context"), None);
    }

    #[test]
    fn subagent_heavy_by_turn_count_and_by_cost_share() {
        // s1: 3 sidechain turns          → heavy by count.
        // s2: 1 sidechain turn, >50% cost → heavy by cost share.
        // s3: 1 cheap sidechain turn among expensive main-thread ones → neither.
        let a = run(
            "subagents",
            &[
                Turn::new("s1", "a1").sidechain(),
                Turn::new("s1", "a2").sidechain(),
                Turn::new("s1", "a3").sidechain(),
                Turn::new("s2", "b1").tokens(0, 1000, 0, 0).sidechain(),
                Turn::new("s2", "b2").tokens(0, 10, 0, 0),
                Turn::new("s3", "c1").tokens(0, 1, 0, 0).sidechain(),
                Turn::new("s3", "c2").tokens(0, 1000, 0, 0),
            ],
            &[],
        );
        let (_, sessions) = behavior(&a.week, "subagent_heavy").expect("subagent_heavy");
        assert_eq!(sessions, 2, "s1 by count, s2 by cost share, s3 neither");
        assert_eq!(a.week.session_count, 3);
    }

    #[test]
    fn long_running_sessions_need_eight_distinct_hours() {
        // Eight turns exactly an hour apart span eight absolute hour buckets.
        let reqs = ["l0", "l1", "l2", "l3", "l4", "l5", "l6", "l7"];
        let turns: Vec<Turn> = (0..8i64)
            .zip(reqs)
            .map(|(i, r)| Turn {
                req: r,
                ..Turn::new("long", "x").at(i * 60 + 5)
            })
            .collect();
        let a = run("cron", &turns, &[]);
        let (pct, sessions) = behavior(&a.week, "cron").expect("cron");
        assert_eq!(sessions, 1);
        assert_eq!(pct, 100);

        // A session confined to one hour is not long-running.
        let a = run(
            "nocron",
            &[
                Turn::new("short", "s0").at(1),
                Turn::new("short", "s1").at(2),
            ],
            &[],
        );
        assert_eq!(behavior(&a.week, "cron"), None);
    }

    #[test]
    fn parallel_sessions_need_four_in_one_bucket() {
        let a = run(
            "parallel",
            &[
                Turn::new("p1", "p1").at(1),
                Turn::new("p2", "p2").at(1),
                Turn::new("p3", "p3").at(1),
                Turn::new("p4", "p4").at(1),
            ],
            &[],
        );
        let (pct, count) = behavior(&a.week, "high_parallel").expect("high_parallel");
        assert_eq!(count, 4, "requests in the bucket, not sessions");
        assert_eq!(pct, 100);

        // Three is not enough.
        let a = run(
            "notparallel",
            &[
                Turn::new("q1", "q1").at(1),
                Turn::new("q2", "q2").at(1),
                Turn::new("q3", "q3").at(1),
            ],
            &[],
        );
        assert_eq!(behavior(&a.week, "high_parallel"), None);
    }

    #[test]
    fn a_skill_inside_a_subagent_is_attributed_to_the_subagent_only() {
        let a = run(
            "attribution",
            &[
                // Both fields set → agents only, keyed by the skill name.
                Turn::new("s1", "r1")
                    .tokens(0, 100, 0, 0)
                    .extra(r#""attributionAgent":"Explore","attributionSkill":"code-review""#),
                // Agent alone → agents, keyed by the agent name.
                Turn::new("s1", "r2")
                    .tokens(0, 100, 0, 0)
                    .extra(r#""attributionAgent":"Plan""#),
                // Skill alone → skills.
                Turn::new("s1", "r3")
                    .tokens(0, 100, 0, 0)
                    .extra(r#""attributionSkill":"run""#),
                // Plugin and MCP server are tracked either way.
                Turn::new("s1", "r4").tokens(0, 100, 0, 0).extra(
                    r#""attributionMcpServer":"azure-devops","attributionPlugin":"acme""#,
                ),
            ],
            &[],
        );
        assert_eq!(share(&a.week.agents, "code-review"), Some(25));
        assert_eq!(share(&a.week.agents, "Plan"), Some(25));
        assert_eq!(
            share(&a.week.skills, "code-review"),
            None,
            "a skill under a subagent must not double-count in skills"
        );
        assert_eq!(share(&a.week.skills, "run"), Some(25));
        assert_eq!(share(&a.week.mcp_servers, "azure-devops"), Some(25));
        assert_eq!(share(&a.week.plugins, "acme"), Some(25));
    }

    #[test]
    fn model_tier_scales_the_weight() {
        // Identical tokens; fable weighs 10x haiku → 91% / 9%.
        let a = run(
            "tiers",
            &[
                Turn::new("s1", "f")
                    .tokens(0, 100, 0, 0)
                    .model("claude-fable-5-1")
                    .extra(r#""attributionSkill":"fable""#),
                Turn::new("s1", "h")
                    .tokens(0, 100, 0, 0)
                    .model("claude-haiku-4-5")
                    .extra(r#""attributionSkill":"haiku""#),
            ],
            &[],
        );
        assert_eq!(share(&a.week.skills, "fable"), Some(91));
        assert_eq!(share(&a.week.skills, "haiku"), Some(9));

        assert_eq!(model_tier("claude-opus-5[1m]"), 5.0);
        assert_eq!(model_tier("claude-sonnet-5"), 3.0);
        assert_eq!(model_tier("<synthetic>"), 3.0);
        assert_eq!(model_tier(""), 3.0);
    }

    #[test]
    fn tiny_shares_and_quiet_behaviors_are_dropped() {
        let a = run(
            "small",
            &[
                Turn::new("s1", "big")
                    .tokens(0, 100_000, 0, 0)
                    .extra(r#""attributionSkill":"big""#),
                Turn::new("s1", "tiny")
                    .tokens(0, 100, 0, 0)
                    .extra(r#""attributionSkill":"tiny""#),
            ],
            &[],
        );
        assert_eq!(share(&a.week.skills, "big"), Some(100));
        assert_eq!(share(&a.week.skills, "tiny"), None, "0% shares are dropped");

        // A cold cache worth well under 10% of usage is not worth reporting.
        let a = run(
            "quiet",
            &[
                Turn::new("s1", "miss").tokens(100_001, 0, 0, 0),
                Turn::new("s1", "bulk").tokens(0, 10_000_000, 0, 0),
            ],
            &[],
        );
        assert_eq!(behavior(&a.week, "cache_miss"), None);
    }

    #[test]
    fn turns_are_deduped_on_request_id() {
        // Same requestId, different message id → one turn.
        let a = run(
            "dedup",
            &[
                Turn::new("s1", "same").tokens(0, 100, 0, 0),
                Turn::new("s1", "same").tokens(0, 100, 0, 0),
            ],
            &[],
        );
        assert_eq!(a.week.request_count, 1);
        assert_eq!(a.sessions[0].turns, 1);
        assert_eq!(a.sessions[0].output, 100);
    }

    #[test]
    fn zero_token_turns_are_skipped() {
        let a = run(
            "zero",
            &[
                Turn::new("s1", "zero").tokens(0, 0, 0, 0),
                Turn::new("s1", "real").tokens(0, 5, 0, 0),
            ],
            &[],
        );
        assert_eq!(a.week.request_count, 1);
    }

    #[test]
    fn day_window_excludes_older_turns() {
        let a = run(
            "windows",
            &[
                Turn::new("s1", "recent").tokens(0, 100, 0, 0),
                // Three days ago — inside the week, outside the day.
                Turn::new("s2", "old").tokens(0, 100, 0, 0).at(3 * 24 * 60),
            ],
            &[],
        );
        assert_eq!(a.week.request_count, 2);
        assert_eq!(a.week.session_count, 2);
        assert_eq!(a.day.request_count, 1);
        assert_eq!(a.day.session_count, 1);
    }

    #[test]
    fn cost_state_supplies_the_real_dollar_figure() {
        let start = Utc::now().timestamp_millis() - 3_600_000;
        let cost_state = format!(
            r#"{{"type":"cost-state","sessionId":"s1","totalCostUSD":26.817331,"totalAPIDuration":2051682,"totalToolDuration":627372,"totalDuration":78140655,"totalLinesAdded":1854,"totalLinesRemoved":761,"startTime":{start},"modelUsage":{{"claude-opus-5[1m]":{{"costUSD":26.8}}}}}}"#
        );
        let title = r#"{"type":"ai-title","aiTitle":"PR review opening","sessionId":"s1"}"#;
        let a = run(
            "coststate",
            &[
                Turn::new("s1", "r1").tokens(0, 100, 0, 0),
                // A second, still-live session with no cost-state record.
                Turn::new("s2", "r2").tokens(0, 100, 0, 0).extra(
                    r#""slug":"my-slug","cwd":"/home/u/projects/widget","gitBranch":"main""#,
                ),
            ],
            &[cost_state, title.to_string()],
        );

        let s1 = a.sessions.iter().find(|s| s.id == "s1").expect("s1");
        assert!(s1.has_cost_state);
        assert!((s1.cost - 26.817331).abs() < 1e-9);
        assert_eq!(s1.api_ms, 2_051_682);
        assert_eq!(s1.wall_ms, 78_140_655);
        assert_eq!(s1.tool_ms, 627_372);
        assert_eq!(s1.lines_added, 1854);
        assert_eq!(s1.lines_removed, 761);
        assert_eq!(s1.started_at, start, "cost-state startTime wins");
        assert_eq!(s1.label, "PR review opening", "aiTitle is the best label");
        assert!(s1.models.iter().any(|m| m == "claude-opus-5[1m]"));

        let s2 = a.sessions.iter().find(|s| s.id == "s2").expect("s2");
        assert!(!s2.has_cost_state, "a live session has no cost record yet");
        assert_eq!(s2.cost, 0.0, "and nothing is estimated in its place");
        assert_eq!(s2.wall_ms, 0);
        assert_eq!(s2.label, "my-slug", "slug is the fallback label");
        assert_eq!(s2.project, "widget", "cwd basename beats the project dir");
        assert_eq!(s2.branch, "main");
        assert!(s2.started_at > 0, "falls back to the first turn");
    }

    #[test]
    fn session_label_falls_back_to_a_short_id() {
        let a = run(
            "label",
            &[Turn::new("0123456789abcdef", "r1").tokens(0, 100, 0, 0)],
            &[],
        );
        assert_eq!(a.sessions[0].label, "01234567");
        assert_eq!(a.sessions[0].project, "a-project", "the project dir name");
    }

    #[test]
    fn sessions_are_newest_first() {
        let a = run(
            "order",
            &[
                Turn::new("older", "o").at(600),
                Turn::new("newest", "n").at(1),
                Turn::new("middle", "m").at(60),
            ],
            &[],
        );
        let ids: Vec<&str> = a.sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["newest", "middle", "older"]);
    }
}

#[cfg(test)]
mod smoke {
    use super::*;

    /// Prints the real attribution for one environment so it can be diffed
    /// against what `/usage` shows in Claude Code. Ignored by default (it reads
    /// the developer's own transcripts and has no fixed expectations).
    ///
    ///   cargo test attribution::smoke -- --ignored --nocapture
    ///   CLAUDE_USAGE_SMOKE_ENV=.claude-team cargo test attribution::smoke -- --ignored --nocapture
    #[test]
    #[ignore]
    fn smoke_real_env() {
        let id = std::env::var("CLAUDE_USAGE_SMOKE_ENV").unwrap_or_else(|_| ".claude".to_string());
        let root = dirs::home_dir().map(|h| h.join(&id).join("projects"));
        println!("env {id} -> {root:?}");
        let a = collect_at(root);
        println!(
            "found={} scanned_files={} sessions={}",
            a.found,
            a.scanned_files,
            a.sessions.len()
        );
        for (label, w) in [("24h", &a.day), ("7d", &a.week)] {
            println!(
                "\n--- {label}: requests={} sessions={}",
                w.request_count, w.session_count
            );
            for b in &w.behaviors {
                println!("  {:<16} {:>3}%  (n={})", b.key, b.pct, b.count);
            }
            for (title, rows) in [
                ("skills", &w.skills),
                ("agents", &w.agents),
                ("mcp", &w.mcp_servers),
                ("plugins", &w.plugins),
            ] {
                if rows.is_empty() {
                    continue;
                }
                let joined: Vec<String> = rows
                    .iter()
                    .take(8)
                    .map(|s| format!("{} {}%", s.name, s.pct))
                    .collect();
                println!("  {title:<8} (n={}) {}", rows.len(), joined.join(", "));
            }
        }
        println!("\n--- sessions (newest first)");
        for s in a.sessions.iter().take(8) {
            println!(
                "  {:>8}  turns={:<5} cost={:<9} wall={:<9} {} / {}",
                if s.has_cost_state { "real" } else { "live" },
                s.turns,
                if s.has_cost_state {
                    format!("${:.2}", s.cost)
                } else {
                    "-".to_string()
                },
                format!("{}m", s.wall_ms / 60_000),
                s.project,
                s.label
            );
        }
    }
}
