//! Reads Claude Code / Claude Desktop usage logs from `~/.claude/projects`
//! and aggregates them into the shape the widget renders.
//!
//! Each `*.jsonl` file is a session transcript. We only care about
//! `type == "assistant"` lines, which carry `message.usage` token counts and
//! `message.model`. Retried/resumed turns are de-duplicated on
//! `(requestId, message.id)`.

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike, Utc};
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
    pub weekly: Weekly,
    pub today: Today,
    pub month: Month,

    pub models: Vec<ModelStat>,
    pub surfaces: Vec<SurfaceStat>,
    pub burn: Vec<u32>,
    pub daily: Vec<DailyStat>,
    pub recent: Vec<RecentTask>,
    /// 7 rows (days, oldest→newest) × 24 cols (hour of day), intensity 0..1.
    pub heatmap: Vec<HeatRow>,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// ms epoch when the current 5-hour block started.
    pub started_at: i64,
    pub prompts: u32,
    pub tokens: u64,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Weekly {
    /// ms epoch of the 7-day window start.
    pub started_at: i64,
    pub prompts: u32,
    pub tokens: u64,
    pub opus_prompts: u32,
    pub opus_tokens: u64,
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
    pub id: String,   // "opus" | "sonnet" | "haiku"
    pub name: String, // family label
    pub tokens: u64,
    pub cost: f64,
    pub prompts: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceStat {
    pub id: String, // "code" | "chat" | "other"
    pub name: String,
    pub prompts: u32,
    pub tokens: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyStat {
    pub label: String, // weekday initials, e.g. "Mon"
    pub code: u32,
    pub chat: u32,
    pub other: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentTask {
    pub task: String,
    pub tokens: u64,
    pub model: String, // family id
    pub t: String,     // relative time, e.g. "14m ago"
    pub ts: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatRow {
    pub label: String,
    pub cells: Vec<f64>, // 24 values, 0..1
}

// ─── Internal per-turn record ──────────────────────────────────────────────

struct Turn {
    ts: DateTime<Utc>,
    family: Family,
    tokens: u64, // input + output + cache (all token movement)
    cost: f64,
    surface: &'static str, // "code" | "chat" | "other"
    session_id: String,
}

fn surface_of(entrypoint: &str, _cwd: &str) -> &'static str {
    let e = entrypoint.to_ascii_lowercase();
    // Claude Code: "cli", "sdk-cli", "claude-code", or older logs with no
    // entrypoint field (those predate the field and are all CLI/Code sessions).
    if e.is_empty() || e.contains("cli") || e.contains("code") {
        "code"
    } else if e.contains("desktop") || e.contains("chat") || e.contains("claude.ai") || e.contains("web") {
        "chat"
    } else {
        "other"
    }
}

fn surface_name(id: &str) -> &'static str {
    match id {
        "code" => "Claude Code",
        "chat" => "Claude Desktop",
        _ => "Other",
    }
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim().replace('\n', " ");
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s
    } else {
        let mut out: String = chars[..max].iter().collect();
        out.push('…');
        out
    }
}

fn rel_time(ts: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - ts).num_seconds().max(0);
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m > 0 {
            format!("{}h {}m", h, m)
        } else {
            format!("{}h ago", h)
        }
    } else {
        format!("{}d ago", secs / 86_400)
    }
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
    collect_at(logs_root())
}

/// Walk a specific `projects/` directory and build the full UsageData. Returns a
/// mock-flagged empty struct (is_mock=true) when there is nothing to read.
pub fn collect_at(root: Option<std::path::PathBuf>) -> UsageData {
    let root = match root {
        Some(r) if r.exists() => r,
        _ => return empty_mock(),
    };

    let now = Utc::now();
    let mut seen: HashSet<String> = HashSet::new();
    let mut turns: Vec<Turn> = Vec::new();
    // session_id -> (earliest user ts, first user message text)
    let mut first_user: HashMap<String, (DateTime<Utc>, String)> = HashMap::new();

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
            let typ = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
            let ts = match v.get("timestamp").and_then(|x| x.as_str()) {
                Some(s) => match DateTime::parse_from_rfc3339(s) {
                    Ok(t) => t.with_timezone(&Utc),
                    Err(_) => continue,
                },
                None => continue,
            };
            let session_id = v
                .get("sessionId")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();

            if typ == "user" {
                // Capture earliest user message text per session for "recent tasks".
                if let Some(text) = extract_user_text(&v) {
                    let entry = first_user
                        .entry(session_id.clone())
                        .or_insert((ts, text.clone()));
                    if ts < entry.0 {
                        *entry = (ts, text);
                    }
                }
                continue;
            }
            if typ != "assistant" {
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
            let family = Family::from_model(model);

            let g = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
            let input = g("input_tokens");
            let output = g("output_tokens");
            let cache_w = g("cache_creation_input_tokens");
            let cache_r = g("cache_read_input_tokens");

            let cost = turn_cost(family, input, output, cache_w, cache_r);
            let tokens = input + output + cache_w + cache_r;

            let entrypoint = v.get("entrypoint").and_then(|x| x.as_str()).unwrap_or("");
            let cwd = v.get("cwd").and_then(|x| x.as_str()).unwrap_or("");
            let surface = surface_of(entrypoint, cwd);

            turns.push(Turn {
                ts,
                family,
                tokens,
                cost,
                surface,
                session_id,
            });
        }
    }

    if turns.is_empty() {
        return empty_mock();
    }

    turns.sort_by_key(|t| t.ts);
    aggregate(turns, first_user, now)
}

fn extract_user_text(v: &serde_json::Value) -> Option<String> {
    let content = v.get("message")?.get("content")?;
    let text = match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .find_map(|b| b.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
            .unwrap_or_default(),
        _ => String::new(),
    };
    let text = text.trim();
    // Skip tool-result / command noise.
    if text.is_empty() || text.starts_with("<") || text.starts_with("[") {
        None
    } else {
        Some(text.to_string())
    }
}

fn aggregate(
    turns: Vec<Turn>,
    first_user: HashMap<String, (DateTime<Utc>, String)>,
    now: DateTime<Utc>,
) -> UsageData {
    let week_start = now - Duration::days(7);
    let now_local = Local::now();
    let today_local = now_local.date_naive();
    let month_start = Local
        .with_ymd_and_hms(now_local.year(), now_local.month(), 1, 0, 0, 0)
        .single()
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);

    // ── 5-hour blocks (ccusage style: split on >5h since block start or gap) ──
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
    let active_block_start = block_start;

    // ── Accumulators ──
    let mut session = Window {
        started_at: active_block_start.timestamp_millis(),
        ..Default::default()
    };
    let mut weekly = Weekly {
        started_at: week_start.timestamp_millis(),
        ..Default::default()
    };
    let mut today = Today::default();
    let mut month = Month { started_at: month_start, ..Default::default() };

    let mut model_map: HashMap<&'static str, ModelStat> = HashMap::new();
    let mut surf_map: HashMap<&'static str, SurfaceStat> = HashMap::new();
    let mut burn = vec![0u32; 20];
    // daily: last 7 calendar days keyed by date.
    let mut daily_map: HashMap<chrono::NaiveDate, DailyStat> = HashMap::new();
    // session_id -> (tokens, family, last_ts)
    let mut sess_tokens: HashMap<String, (u64, Family, DateTime<Utc>)> = HashMap::new();
    // heatmap: date -> [24] counts
    let mut heat_map: HashMap<chrono::NaiveDate, [f64; 24]> = HashMap::new();

    let burn_window_start = now - Duration::seconds(FIVE_HOURS);

    for t in &turns {
        // session (current 5h block)
        if t.ts >= active_block_start {
            session.prompts += 1;
            session.tokens += t.tokens;
        }
        // weekly
        if t.ts >= week_start {
            weekly.prompts += 1;
            weekly.tokens += t.tokens;
            if t.family == Family::Opus {
                weekly.opus_prompts += 1;
                weekly.opus_tokens += t.tokens;
            }
            // models (last 7 days)
            let id = fam_id(t.family);
            let m = model_map.entry(id).or_insert_with(|| ModelStat {
                id: id.to_string(),
                name: t.family.label().to_string(),
                tokens: 0,
                cost: 0.0,
                prompts: 0,
            });
            m.tokens += t.tokens;
            m.cost += t.cost;
            m.prompts += 1;

            // surfaces (last 7 days)
            let s = surf_map.entry(t.surface).or_insert_with(|| SurfaceStat {
                id: t.surface.to_string(),
                name: surface_name(t.surface).to_string(),
                prompts: 0,
                tokens: 0,
            });
            s.prompts += 1;
            s.tokens += t.tokens;

            // daily split
            let d = t.ts.with_timezone(&Local).date_naive();
            let day = daily_map.entry(d).or_insert_with(|| DailyStat {
                label: weekday_label(d),
                code: 0,
                chat: 0,
                other: 0,
            });
            match t.surface {
                "code" => day.code += 1,
                "chat" => day.chat += 1,
                _ => day.other += 1,
            }

            // heatmap
            let hh = t.ts.with_timezone(&Local).hour() as usize;
            heat_map.entry(d).or_insert([0.0; 24])[hh] += 1.0;
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

        // burn (last 5h, 15-min bins)
        if t.ts >= burn_window_start {
            let secs = (t.ts - burn_window_start).num_seconds();
            let bin = ((secs / (15 * 60)) as usize).min(19);
            burn[bin] += 1;
        }

        // recent task accumulation
        let e = sess_tokens
            .entry(t.session_id.clone())
            .or_insert((0, t.family, t.ts));
        e.0 += t.tokens;
        e.2 = t.ts;
    }

    // ── models: drop empty buckets, stable order opus/sonnet/haiku then others ──
    let mut models: Vec<ModelStat> = model_map.into_values().filter(|m| m.tokens > 0).collect();
    models.sort_by_key(|m| model_order(&m.id));

    // ── surfaces: stable order code/chat/other ──
    let mut surfaces: Vec<SurfaceStat> = surf_map.into_values().collect();
    surfaces.sort_by_key(|s| match s.id.as_str() {
        "code" => 0,
        "chat" => 1,
        _ => 2,
    });

    // ── daily: last 7 days ending today, in order ──
    let mut daily = Vec::with_capacity(7);
    for i in (0..7i64).rev() {
        let d = today_local - Duration::days(i);
        daily.push(daily_map.remove(&d).unwrap_or(DailyStat {
            label: weekday_label(d),
            code: 0,
            chat: 0,
            other: 0,
        }));
    }

    // ── heatmap: last 7 days, normalized to global max ──
    let global_max = heat_map
        .values()
        .flat_map(|row| row.iter().cloned())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let mut heatmap = Vec::with_capacity(7);
    for i in (0..7i64).rev() {
        let d = today_local - Duration::days(i);
        let raw = heat_map.get(&d).cloned().unwrap_or([0.0; 24]);
        let cells = raw.iter().map(|c| (c / global_max).min(1.0)).collect();
        heatmap.push(HeatRow {
            label: weekday_initial(d),
            cells,
        });
    }

    // ── recent tasks: most recent sessions with a user message ──
    let mut recent: Vec<RecentTask> = sess_tokens
        .into_iter()
        .filter_map(|(sid, (tokens, fam, last))| {
            let (_, text) = first_user.get(&sid)?;
            Some(RecentTask {
                task: truncate(text, 38),
                tokens,
                model: fam_id(fam).to_string(),
                t: rel_time(last, now),
                ts: last.timestamp_millis(),
            })
        })
        .collect();
    recent.sort_by(|a, b| b.ts.cmp(&a.ts));
    recent.truncate(5);

    UsageData {
        is_mock: false,
        generated_at: now.timestamp_millis(),
        session,
        weekly,
        today,
        month,
        models,
        surfaces,
        burn,
        daily,
        recent,
        heatmap,
    }
}

fn fam_id(f: Family) -> &'static str {
    match f {
        Family::Opus => "opus",
        Family::Sonnet => "sonnet",
        Family::Haiku => "haiku",
        Family::Other => "other",
    }
}

fn model_order(id: &str) -> u8 {
    match id {
        "opus" => 0,
        "sonnet" => 1,
        "haiku" => 2,
        _ => 3,
    }
}

fn weekday_label(d: chrono::NaiveDate) -> String {
    match d.weekday() {
        chrono::Weekday::Mon => "Mon",
        chrono::Weekday::Tue => "Tue",
        chrono::Weekday::Wed => "Wed",
        chrono::Weekday::Thu => "Thu",
        chrono::Weekday::Fri => "Fri",
        chrono::Weekday::Sat => "Sat",
        chrono::Weekday::Sun => "Sun",
    }
    .to_string()
}

fn weekday_initial(d: chrono::NaiveDate) -> String {
    weekday_label(d).chars().next().unwrap_or('?').to_string()
}

/// Empty placeholder telling the frontend to use its bundled mock data.
fn empty_mock() -> UsageData {
    UsageData {
        is_mock: true,
        generated_at: Utc::now().timestamp_millis(),
        ..Default::default()
    }
}

// Silence unused-import warning for TimeZone/Local::timestamp paths on some platforms.
#[allow(dead_code)]
fn _touch_timezone() {
    let _ = Local.timestamp_opt(0, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn write_fixture(dir: &std::path::Path) {
        let ts = Utc::now().to_rfc3339();
        let mut f = std::fs::File::create(dir.join("session.jsonl")).unwrap();
        let lines = [
            format!(r#"{{"type":"user","timestamp":"{ts}","sessionId":"s1","message":{{"role":"user","content":"refactor the auth flow"}}}}"#),
            // opus turn
            format!(r#"{{"type":"assistant","timestamp":"{ts}","sessionId":"s1","requestId":"r1","entrypoint":"cli","cwd":"C:/x","message":{{"id":"m1","model":"claude-opus-4-7","usage":{{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000}}}}}}"#),
            // exact duplicate (same requestId+id) — must be deduped
            format!(r#"{{"type":"assistant","timestamp":"{ts}","sessionId":"s1","requestId":"r1","entrypoint":"cli","cwd":"C:/x","message":{{"id":"m1","model":"claude-opus-4-7","usage":{{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000}}}}}}"#),
            // haiku turn on the desktop surface
            format!(r#"{{"type":"assistant","timestamp":"{ts}","sessionId":"s1","requestId":"r2","entrypoint":"claude-desktop","cwd":"C:/x","message":{{"id":"m2","model":"claude-haiku-4-5","usage":{{"input_tokens":5,"output_tokens":5,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#),
        ];
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
    }

    #[test]
    fn aggregates_dedups_and_buckets() {
        let dir = temp_subdir("agg");
        write_fixture(&dir);
        let u = collect_at(Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);

        assert!(!u.is_mock);
        // 2 unique assistant turns (the duplicate r1/m1 was deduped)
        assert_eq!(u.today.prompts, 2);
        assert_eq!(u.session.prompts, 2);

        // models: opus + haiku, opus tokens are the full sum of all four buckets
        assert_eq!(u.models.len(), 2);
        let opus = u.models.iter().find(|m| m.id == "opus").unwrap();
        assert_eq!(opus.tokens, 10 + 20 + 100 + 1000);
        assert!(opus.cost > 0.0);

        // surfaces: cli -> code, claude-desktop -> chat
        assert_eq!(u.surfaces.iter().find(|s| s.id == "code").unwrap().prompts, 1);
        assert_eq!(u.surfaces.iter().find(|s| s.id == "chat").unwrap().prompts, 1);

        // recent task is labeled from the first user message
        assert!(u.recent.iter().any(|r| r.task.contains("refactor the auth flow")));
    }

    #[test]
    fn empty_dir_returns_mock_flag() {
        let dir = temp_subdir("empty");
        let u = collect_at(Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(u.is_mock);
    }

    #[test]
    fn surface_mapping() {
        assert_eq!(surface_of("cli", "C:/x"), "code");
        assert_eq!(surface_of("sdk-cli", "C:/x"), "code");
        assert_eq!(surface_of("", "C:/x"), "code"); // older logs w/o entrypoint
        assert_eq!(surface_of("claude-desktop", ""), "chat");
    }
}
