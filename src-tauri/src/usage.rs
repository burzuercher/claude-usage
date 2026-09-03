//! Reads Claude Code / Claude Desktop usage logs from `~/.claude*/projects`
//! and aggregates them into the shape the widget renders.
//!
//! Each `*.jsonl` file is a session transcript. We only care about
//! `type == "assistant"` lines, which carry `message.usage` token counts and
//! `message.model`. Retried/resumed turns are de-duplicated on
//! `(requestId, message.id)`.
//!
//! Usage *limits* (ring, weekly bars) come from Anthropic's usage API, not from
//! here — this module supplies the factual local data: tokens, estimated cost,
//! and, for the current session window, the per-model split and the token burn
//! per 15-minute bin (the sparkline + the pace forecast).

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use walkdir::WalkDir;

use crate::pricing::{turn_cost, Family};

const FIVE_HOURS: i64 = 5 * 3600;

// ─── Output shapes (camelCase for the frontend) ────────────────────────────

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageData {
    pub is_mock: bool,
    pub generated_at: i64,

    pub session: Window,
    pub today: Today,
    pub month: Month,

    /// Per-model split for the current session window.
    pub models: Vec<ModelStat>,
    /// Tokens per 15-minute bin across the session window (20 bins = 5h),
    /// indexed from the window start; bins after "now" are still 0.
    pub burn: Vec<u64>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// ms epoch when the current session window started.
    pub started_at: i64,
    /// True when the window start came from the live usage API (matches the ring).
    pub from_live: bool,
    pub prompts: u32,
    pub tokens: u64,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Today {
    pub tokens: u64,
    pub cost: f64,
    pub prompts: u32,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Month {
    /// ms epoch of the first day of the current calendar month (local).
    pub started_at: i64,
    pub tokens: u64,
    pub cost: f64,
    pub prompts: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStat {
    pub id: String,   // "fable" | "opus" | "sonnet" | "haiku" | "other"
    pub name: String, // family label
    pub tokens: u64,
    pub cost: f64,
    pub prompts: u32,
}

// ─── Internal per-turn record ──────────────────────────────────────────────

struct Turn {
    ts: DateTime<Utc>,
    family: Family,
    tokens: u64, // input + output + cache (all token movement)
    cost: f64,
}

/// Resolve the Claude logs root. Honors `CLAUDE_USAGE_LOG_DIR` for testing.
#[allow(dead_code)]
fn logs_root() -> Option<std::path::PathBuf> {
    if let Ok(custom) = std::env::var("CLAUDE_USAGE_LOG_DIR") {
        return Some(std::path::PathBuf::from(custom));
    }
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

/// Walk the default logs root (env override or `~/.claude/projects`). Used by
/// the smoke test; the app drives `collect_at` with a chosen environment.
#[allow(dead_code)]
pub fn collect() -> UsageData {
    collect_at(logs_root(), None)
}

/// Walk a specific `projects/` directory and build the full UsageData.
///
/// `session_start_ms` is the live session window start (reset time − 5h) when
/// the frontend has it; otherwise we fall back to a ccusage-style local 5-hour
/// block. Returns a mock-flagged empty struct when there is nothing to read.
pub fn collect_at(root: Option<std::path::PathBuf>, session_start_ms: Option<i64>) -> UsageData {
    let root = match root {
        Some(r) if r.exists() => r,
        _ => return empty_mock(),
    };

    let now = Utc::now();
    let mut seen: HashSet<String> = HashSet::new();
    let mut turns: Vec<Turn> = Vec::new();

    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
    {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for line in content.lines() {
            if line.is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(|x| x.as_str()) != Some("assistant") {
                continue;
            }
            let ts = match v.get("timestamp").and_then(|x| x.as_str()) {
                Some(s) => match DateTime::parse_from_rfc3339(s) {
                    Ok(t) => t.with_timezone(&Utc),
                    Err(_) => continue,
                },
                None => continue,
            };

            let msg = match v.get("message") {
                Some(m) => m,
                None => continue,
            };
            let usage = match msg.get("usage") {
                Some(u) => u,
                None => continue,
            };

            // Dedup on (requestId, message.id), falling back to uuid.
            let req = v.get("requestId").and_then(|x| x.as_str()).unwrap_or("");
            let mid = msg.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let key = if !req.is_empty() || !mid.is_empty() {
                format!("{req}:{mid}")
            } else {
                v.get("uuid")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            if !key.is_empty() && !seen.insert(key) {
                continue;
            }

            let model = msg.get("model").and_then(|x| x.as_str()).unwrap_or("");
            let (tokens, cost) = turn_tokens_and_cost(model, usage);

            turns.push(Turn {
                ts,
                family: Family::from_model(model),
                tokens,
                cost,
            });
        }
    }

    if turns.is_empty() {
        return empty_mock();
    }

    turns.sort_by_key(|t| t.ts);
    aggregate(turns, session_start_ms, now)
}

/// Total token movement and estimated cost for one turn's `usage` object.
///
/// Newer logs split cache writes by TTL under `cache_creation`; when that
/// object is absent, all of `cache_creation_input_tokens` is billed as 5-minute.
fn turn_tokens_and_cost(model: &str, usage: &serde_json::Value) -> (u64, f64) {
    let g = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    let input = g("input_tokens");
    let output = g("output_tokens");
    let cache_w_total = g("cache_creation_input_tokens");
    let cache_r = g("cache_read_input_tokens");

    let (cache_w_5m, cache_w_1h) = match usage.get("cache_creation") {
        Some(cc) if cc.is_object() => {
            let c = |k: &str| cc.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
            let one_h = c("ephemeral_1h_input_tokens");
            let five_m = c("ephemeral_5m_input_tokens");
            // Guard against a partial breakdown: anything unaccounted for is 5m.
            let rest = cache_w_total.saturating_sub(one_h + five_m);
            (five_m + rest, one_h)
        }
        _ => (cache_w_total, 0),
    };

    let cost = turn_cost(model, input, output, cache_w_5m, cache_w_1h, cache_r);
    (input + output + cache_w_total + cache_r, cost)
}

fn aggregate(turns: Vec<Turn>, session_start_ms: Option<i64>, now: DateTime<Utc>) -> UsageData {
    let now_local = Local::now();
    let today_local = now_local.date_naive();
    let month_start = Local
        .with_ymd_and_hms(now_local.year(), now_local.month(), 1, 0, 0, 0)
        .single()
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);

    // ── Session window: live start when known, else a ccusage-style local
    //    5-hour block (split on >5h since block start or a >5h gap). ──
    let live_start = session_start_ms.and_then(|ms| Utc.timestamp_millis_opt(ms).single());
    let session_start = match live_start {
        Some(s) => s,
        None => {
            let mut block_start = turns[0].ts;
            let mut last_ts = turns[0].ts;
            for t in &turns {
                let since_start = (t.ts - block_start).num_seconds();
                let gap = (t.ts - last_ts).num_seconds();
                if since_start > FIVE_HOURS || gap > FIVE_HOURS {
                    block_start = t.ts;
                }
                last_ts = t.ts;
            }
            // A block that went idle for 5h+ is over: report an empty session
            // rather than presenting stale turns as "this session".
            if (now - last_ts).num_seconds() > FIVE_HOURS {
                now
            } else {
                block_start
            }
        }
    };

    // ── Accumulators ──
    let mut session = Window {
        started_at: session_start.timestamp_millis(),
        from_live: live_start.is_some(),
        ..Default::default()
    };
    let mut today = Today::default();
    let mut month = Month { started_at: month_start, ..Default::default() };
    let mut model_map: HashMap<Family, ModelStat> = HashMap::new();
    let mut burn = vec![0u64; 20];

    for t in &turns {
        // session window: totals, per-model split, and 15-min burn bins
        if t.ts >= session_start {
            session.prompts += 1;
            session.tokens += t.tokens;
            let bin = ((t.ts - session_start).num_seconds() / (15 * 60)).clamp(0, 19) as usize;
            burn[bin] += t.tokens;
            let m = model_map.entry(t.family).or_insert_with(|| ModelStat {
                id: t.family.id().to_string(),
                name: t.family.label().to_string(),
                tokens: 0,
                cost: 0.0,
                prompts: 0,
            });
            m.tokens += t.tokens;
            m.cost += t.cost;
            m.prompts += 1;
        }

        // today (local calendar day)
        let tl = t.ts.with_timezone(&Local);
        if tl.date_naive() == today_local {
            today.tokens += t.tokens;
            today.cost += t.cost;
            today.prompts += 1;
        }
        // current calendar month (for expenditures)
        if tl.year() == now_local.year() && tl.month() == now_local.month() {
            month.tokens += t.tokens;
            month.cost += t.cost;
            month.prompts += 1;
        }

    }

    // ── models: drop empty buckets, stable order fable/opus/sonnet/haiku/other ──
    let mut models: Vec<ModelStat> = model_map
        .into_iter()
        .filter(|(_, m)| m.tokens > 0)
        .map(|(_, m)| m)
        .collect();
    models.sort_by_key(|m| Family::from_model(&m.id).order());

    UsageData {
        is_mock: false,
        generated_at: now.timestamp_millis(),
        session,
        today,
        month,
        models,
        burn,
    }
}

/// Empty placeholder telling the frontend to use its bundled mock data.
fn empty_mock() -> UsageData {
    UsageData {
        is_mock: true,
        generated_at: Utc::now().timestamp_millis(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::io::Write;

    fn temp_subdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "cu-{tag}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Two live turns now (fable + opus, plus an exact duplicate of the fable
    /// line) and one opus turn 8 hours ago.
    fn write_fixture(dir: &std::path::Path) {
        let now = Utc::now();
        let ts = now.to_rfc3339();
        let old = (now - Duration::hours(8)).to_rfc3339();
        let mut f = std::fs::File::create(dir.join("session.jsonl")).unwrap();
        let fable = format!(
            r#"{{"type":"assistant","timestamp":"{ts}","sessionId":"s1","requestId":"r1","entrypoint":"cli","message":{{"id":"m1","model":"claude-fable-5-1","usage":{{"input_tokens":1000000,"output_tokens":1000000,"cache_creation_input_tokens":2000000,"cache_read_input_tokens":1000000,"cache_creation":{{"ephemeral_1h_input_tokens":1000000,"ephemeral_5m_input_tokens":1000000}}}}}}}}"#
        );
        let lines = [
            format!(r#"{{"type":"user","timestamp":"{ts}","sessionId":"s1","message":{{"role":"user","content":"refactor the auth flow"}}}}"#),
            fable.clone(),
            // exact duplicate (same requestId+id) — must be deduped
            fable,
            // opus turn, no cache_creation breakdown (legacy shape)
            format!(r#"{{"type":"assistant","timestamp":"{ts}","sessionId":"s1","requestId":"r2","message":{{"id":"m2","model":"claude-opus-5","usage":{{"input_tokens":1000000,"output_tokens":0,"cache_creation_input_tokens":1000000,"cache_read_input_tokens":0}}}}}}"#),
            // old opus turn — outside any live session window starting < 8h ago
            format!(r#"{{"type":"assistant","timestamp":"{old}","sessionId":"s0","requestId":"r0","message":{{"id":"m0","model":"claude-opus-4-7","usage":{{"input_tokens":5,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#),
        ];
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn aggregates_dedups_and_prices_by_model() {
        let dir = temp_subdir("agg");
        write_fixture(&dir);
        let u = collect_at(Some(dir.clone()), None);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!u.is_mock);
        // 2 unique turns today (r1 deduped) + the 8h-old one if today.
        assert!(u.today.prompts >= 2);

        // Local block heuristic: the 8h gap starts a new block → 2 turns in session.
        assert!(!u.session.from_live);
        assert_eq!(u.session.prompts, 2);

        let fable = u.models.iter().find(|m| m.id == "fable").expect("fable bucket");
        // tokens = all four buckets
        assert_eq!(fable.tokens, 1_000_000 + 1_000_000 + 2_000_000 + 1_000_000);
        // cost = 10 (in) + 50 (out) + 12.5 (5m write) + 20 (1h write) + 0.25 (read)
        assert!(close(fable.cost, 92.75), "fable cost {}", fable.cost);

        let opus = u.models.iter().find(|m| m.id == "opus").expect("opus bucket");
        // legacy shape: all cache writes billed as 5m → 5 + 6.25
        assert!(close(opus.cost, 11.25), "opus cost {}", opus.cost);

        // order: fable first
        assert_eq!(u.models[0].id, "fable");
        assert_eq!(u.models[1].id, "opus");
    }

    #[test]
    fn live_session_start_scopes_the_model_split() {
        let dir = temp_subdir("live");
        write_fixture(&dir);
        // Window starting 1 minute ago → only the two "now" turns.
        let start = Utc::now().timestamp_millis() - 60_000;
        let u = collect_at(Some(dir.clone()), Some(start));
        assert!(u.session.from_live);
        assert_eq!(u.session.started_at, start);
        assert_eq!(u.session.prompts, 2);
        // Burn bins are indexed from the window start: both turns land in bin 0.
        assert_eq!(u.burn.len(), 20);
        assert_eq!(u.burn[0], u.session.tokens);
        assert_eq!(u.burn[1..].iter().sum::<u64>(), 0);

        // Window starting 10 hours ago → the old opus turn is included too.
        let start = Utc::now().timestamp_millis() - 10 * 3600 * 1000;
        let u = collect_at(Some(dir.clone()), Some(start));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(u.session.prompts, 3);
        assert_eq!(u.models.iter().find(|m| m.id == "opus").unwrap().prompts, 2);
    }

    #[test]
    fn idle_local_block_yields_an_empty_session() {
        let dir = temp_subdir("idle");
        let old = (Utc::now() - Duration::hours(9)).to_rfc3339();
        let mut f = std::fs::File::create(dir.join("s.jsonl")).unwrap();
        writeln!(f, r#"{{"type":"assistant","timestamp":"{old}","requestId":"r0","message":{{"id":"m0","model":"claude-opus-5","usage":{{"input_tokens":5,"output_tokens":5}}}}}}"#).unwrap();
        let u = collect_at(Some(dir.clone()), None);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!u.is_mock);
        assert_eq!(u.session.prompts, 0);
        assert!(u.models.is_empty());
        assert_eq!(u.burn.iter().sum::<u64>(), 0);
        // ...but the same turn is still counted once a live window covers it.
        let dir = temp_subdir("idle2");
        let mut f = std::fs::File::create(dir.join("s.jsonl")).unwrap();
        writeln!(f, r#"{{"type":"assistant","timestamp":"{old}","requestId":"r0","message":{{"id":"m0","model":"claude-opus-5","usage":{{"input_tokens":5,"output_tokens":5}}}}}}"#).unwrap();
        let u = collect_at(Some(dir.clone()), Some(Utc::now().timestamp_millis() - 10 * 3600 * 1000));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(u.session.prompts, 1);
    }

    #[test]
    fn cache_breakdown_fallbacks() {
        // No cache_creation object → everything is a 5m write.
        let legacy = serde_json::json!({"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":1000000,"cache_read_input_tokens":0});
        let (tok, cost) = turn_tokens_and_cost("claude-opus-5", &legacy);
        assert_eq!(tok, 1_000_000);
        assert!(close(cost, 6.25));

        // Partial breakdown: the unexplained remainder is billed as 5m.
        let partial = serde_json::json!({"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":1000000,"cache_read_input_tokens":0,"cache_creation":{"ephemeral_1h_input_tokens":500000}});
        let (_, cost) = turn_tokens_and_cost("claude-opus-5", &partial);
        assert!(close(cost, 5.0 + 3.125), "cost {cost}");
    }

    #[test]
    fn empty_dir_returns_mock_flag() {
        let dir = temp_subdir("empty");
        let u = collect_at(Some(dir.clone()), None);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(u.is_mock);
    }
}
