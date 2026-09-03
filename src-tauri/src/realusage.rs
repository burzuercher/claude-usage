//! Fetches *authoritative* usage limits from Anthropic — the same data the
//! Claude app shows under "Your usage limits" and Claude Code shows in `/usage`:
//! `GET /api/oauth/usage`.
//!
//! Two sources, same parser:
//!  * **api** — live call with the environment's OAuth access token, read at
//!    runtime from `.credentials.json`. We only use the token if it's still
//!    valid; we never refresh/rotate it (that could disrupt Claude Code's own
//!    login) and never log or return it.
//!  * **cache** — Claude Code persists its own last response in the account file
//!    (`.claude.json` → `cachedUsageUtilization = {fetchedAtMs, utilization}`),
//!    written at most every 5 minutes. We serve it straight when it's fresh
//!    (the endpoint allows one request per token per 2-minute window, shared
//!    with Claude Code) and fall back to it when the API rate-limits us (429),
//!    so the ring degrades to "slightly stale" instead of blank.
//!
//! Temporary limit boosts (e.g. "+50% weekly limits promo through Sep 13") are
//! already baked into the server's utilization %. The *notice* is a GrowthBook
//! feature flag Claude Code caches in the same file
//! (`cachedGrowthBookFeatures.tengu_rate_limit_promo_notices`); we surface it
//! as a note on the matching bar. Grace-window state only travels on message
//! response headers, so it is not observable here.

use chrono::{Datelike, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

use crate::env;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// Serve Claude Code's cached response without a network call when younger than
/// this. The endpoint allows one request per token per 2-minute window, so a
/// response this recent means our own call would be rejected anyway.
const CACHE_FRESH_MS: i64 = 2 * 60 * 1000;
/// Never show a cached response older than this (Claude Code applies the same
/// 1-hour ceiling to its own cache). Beyond it a 5-hour session window has
/// moved on too far for the numbers to mean anything.
const CACHE_MAX_AGE_MS: i64 = 60 * 60 * 1000;

/// Is a cached response (fetched at `fetched_ms`) still worth showing at `now_ms`?
fn cache_usable(fetched_ms: i64, now_ms: i64) -> bool {
    let age = now_ms - fetched_ms;
    (0..CACHE_MAX_AGE_MS).contains(&age)
}

#[derive(Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RealBucket {
    pub key: String,
    pub label: String,
    pub utilization: f64,  // 0..1
    pub resets_at: String, // ISO timestamp, may be empty
    pub severity: String,  // server hint ("normal", …), may be empty
    pub is_active: bool,   // server flag: this window is the binding one
    pub note: String,      // promo / boost notice, may be empty
}

#[derive(Serialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExtraUsage {
    pub is_enabled: bool,
    pub used_dollars: f64,  // extra-usage spend this month, in currency units
    pub limit_dollars: f64, // cap in currency units (0 = none)
    pub currency: String,
    pub utilization: f64, // 0..1 of the extra-usage budget (0 when no cap)
    pub disabled_reason: String,
    pub spend_limit_reached: bool,
    pub can_purchase_credits: bool,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RealUsage {
    pub found: bool,
    pub reason: String,
    /// "api" | "cache" | "" (not found)
    pub source: String,
    pub session: Option<RealBucket>,
    pub weekly: Vec<RealBucket>,
    pub extra: Option<ExtraUsage>,
    /// ms epoch the data was fetched from Anthropic (by us or by Claude Code).
    pub fetched_at: i64,
    /// HTTP status of the last API attempt (0 = not attempted / transport error).
    pub http_status: u16,
    /// Server-suggested retry delay for a 429 (0 = none given).
    pub retry_after_secs: u64,
    /// ms epoch when the promo-notice feature flags were cached (0 = none).
    pub promo_cached_at: i64,
}

fn not_found(reason: &str) -> RealUsage {
    RealUsage {
        found: false,
        reason: reason.to_string(),
        ..Default::default()
    }
}

// ─── Credentials ────────────────────────────────────────────────────────────

/// Read `claudeAiOauth.accessToken` + `expiresAt` from an env's credentials file.
/// Returns Err (with a reason) if missing or expired.
fn load_valid_token(config_dir: &Path) -> Result<String, String> {
    let path = config_dir.join(".credentials.json");
    if !path.exists() {
        return Err("no credentials for this account".into());
    }
    let content = std::fs::read_to_string(&path).map_err(|_| "can't read credentials".to_string())?;
    let v: Value = serde_json::from_str(&content).map_err(|_| "bad credentials json".to_string())?;
    let oauth = v.get("claudeAiOauth").ok_or("no oauth in credentials")?;
    let token = oauth
        .get("accessToken")
        .and_then(|t| t.as_str())
        .ok_or("no access token")?
        .to_string();
    // expiresAt is ms epoch; if present and in the past, treat as expired.
    if let Some(exp) = oauth.get("expiresAt").and_then(|e| e.as_i64()) {
        if exp <= Utc::now().timestamp_millis() {
            return Err("token expired — run Claude Code to refresh".into());
        }
    }
    Ok(token)
}

// ─── Claude Code's account file: cached usage + promo notices ────────────────

fn read_json(path: &Path) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Claude Code's own last usage response: `(fetchedAtMs, utilization body)`.
fn cached_usage(account_json: &Value) -> Option<(i64, Value)> {
    let c = account_json.get("cachedUsageUtilization")?;
    let fetched = c.get("fetchedAtMs").and_then(|x| x.as_i64())?;
    let body = c.get("utilization")?.clone();
    if body.is_object() {
        Some((fetched, body))
    } else {
        None
    }
}

/// Promo / boost notices: `[(bar, text)]` still in effect, plus the flag cache time.
fn promo_notes(account_json: &Value, today: NaiveDate) -> (Vec<(String, String)>, i64) {
    let cached_at = account_json
        .get("cachedGrowthBookFeaturesAt")
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    let notes = account_json
        .get("cachedGrowthBookFeatures")
        .and_then(|f| f.get("tengu_rate_limit_promo_notices"))
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|n| {
                    let bar = n.get("bar")?.as_str()?.trim().to_string();
                    let text = n.get("text")?.as_str()?.trim().to_string();
                    if bar.is_empty() || text.is_empty() || promo_expired(&text, today) {
                        None
                    } else {
                        Some((bar, text))
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    (notes, cached_at)
}

/// Notices carry their own end date in prose ("… through Sep 13 · …"). Hide a
/// notice whose date is in the past; keep anything we can't parse.
fn promo_expired(text: &str, today: NaiveDate) -> bool {
    let lower = text.to_ascii_lowercase();
    let idx = match lower.find("through ") {
        Some(i) => i + "through ".len(),
        None => return false,
    };
    let mut words = lower[idx..].split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty());
    let month = match words.next().and_then(month_number) {
        Some(m) => m,
        None => return false,
    };
    let day: u32 = match words.next().and_then(|w| w.parse().ok()) {
        Some(d) => d,
        None => return false,
    };
    let candidate = match NaiveDate::from_ymd_opt(today.year(), month, day) {
        Some(d) => d,
        None => return false,
    };
    // A date more than ~6 months behind us is really next year's (e.g. a
    // January promo read in December).
    let end = if (today - candidate).num_days() > 183 {
        NaiveDate::from_ymd_opt(today.year() + 1, month, day).unwrap_or(candidate)
    } else {
        candidate
    };
    end < today
}

fn month_number(word: &str) -> Option<u32> {
    let w: String = word.chars().take(3).collect();
    Some(match w.as_str() {
        "jan" => 1, "feb" => 2, "mar" => 3, "apr" => 4, "may" => 5, "jun" => 6,
        "jul" => 7, "aug" => 8, "sep" => 9, "oct" => 10, "nov" => 11, "dec" => 12,
        _ => return None,
    })
}

// ─── Parsing ────────────────────────────────────────────────────────────────

/// Legacy top-level bucket keys we surface, with display label and rank.
/// Anthropic also returns internal buckets (`seven_day_oauth_apps`,
/// `seven_day_cowork`, `cinder_cove` = one-time Claude Code/Cowork credit,
/// `nimbus_quill`, `juniper_tide` = session-reset offer, `omelette_promotional`,
/// `tangelo`, `iguana_necktie`, `amber_ladder`, …) that the Claude app doesn't
/// show as bars, so we allowlist. "omelette" is the codename for Claude Design.
fn legacy_weekly_label(key: &str) -> Option<&'static str> {
    match key {
        "seven_day" => Some("All models"),
        "seven_day_opus" => Some("Opus only"),
        "seven_day_sonnet" => Some("Sonnet only"),
        "seven_day_overage_included" => Some("Fable only"),
        "seven_day_omelette" => Some("Claude Design"),
        _ => None,
    }
}

/// Which bucket a promo notice's `bar` refers to (legacy key → our label).
fn note_target_label(bar: &str) -> Option<&'static str> {
    match bar {
        "five_hour" => Some("Current session"),
        other => legacy_weekly_label(other),
    }
}

fn label_rank(label: &str) -> (u8, String) {
    let l = label.to_ascii_lowercase();
    let rank = if l == "all models" {
        0
    } else if l.starts_with("fable") {
        1
    } else if l.starts_with("opus") {
        2
    } else if l.starts_with("sonnet") {
        3
    } else if l == "claude design" {
        4
    } else {
        5
    };
    (rank, l)
}

/// The usage endpoint reports `utilization` / `percent` as 0..100 (a value of
/// exactly 1 means 1%). Normalize to 0..1 for the UI.
fn norm_util(v: f64) -> f64 {
    (v / 100.0).clamp(0.0, 1.0)
}

/// `resets_at` arrives as an ISO string, or occasionally as epoch seconds.
fn resets_at_iso(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => {
            let secs = n.as_f64().unwrap_or(0.0);
            let ms = if secs > 1e12 { secs } else { secs * 1000.0 };
            chrono::DateTime::from_timestamp_millis(ms as i64)
                .map(|d| d.to_rfc3339())
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

fn str_or_empty(v: Option<&Value>) -> String {
    v.and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// Bucket from a legacy `{utilization, resets_at}` object.
fn legacy_bucket(key: &str, label: &str, obj: &Value) -> Option<RealBucket> {
    let util = obj.get("utilization").and_then(|u| u.as_f64())?;
    Some(RealBucket {
        key: key.to_string(),
        label: label.to_string(),
        utilization: norm_util(util),
        resets_at: resets_at_iso(obj.get("resets_at")),
        severity: str_or_empty(obj.get("severity")),
        is_active: false,
        note: String::new(),
    })
}

/// Bucket from a normalized `limits[]` entry.
fn limit_bucket(item: &Value) -> Option<(bool, RealBucket)> {
    let kind = item.get("kind").and_then(|k| k.as_str())?;
    let percent = item.get("percent").and_then(|p| p.as_f64())?;
    let scope = item.get("scope").filter(|s| s.is_object());
    let model_name = scope
        .and_then(|s| s.get("model"))
        .and_then(|m| m.get("display_name"))
        .and_then(|n| n.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let surface_name = scope
        .and_then(|s| s.get("surface"))
        .and_then(|s| s.as_str().map(str::to_string).or_else(|| s.get("display_name").and_then(|n| n.as_str()).map(str::to_string)))
        .filter(|s| !s.trim().is_empty());

    let (is_session, key, label) = match kind {
        "session" => (true, "five_hour".to_string(), "Current session".to_string()),
        "weekly_all" => (false, "seven_day".to_string(), "All models".to_string()),
        "weekly_scoped" => {
            if let Some(name) = model_name {
                (false, format!("model:{}", name.to_ascii_lowercase()), format!("{name} only"))
            } else if let Some(surface) = surface_name {
                (false, format!("surface:{}", surface.to_ascii_lowercase()), surface)
            } else {
                return None;
            }
        }
        _ => return None,
    };

    Some((
        is_session,
        RealBucket {
            key,
            label,
            utilization: norm_util(percent),
            resets_at: resets_at_iso(item.get("resets_at")),
            severity: str_or_empty(item.get("severity")),
            is_active: item.get("is_active").and_then(|b| b.as_bool()).unwrap_or(false),
            note: String::new(),
        },
    ))
}

/// Money in minor units → currency units.
fn minor_to_units(amount_minor: f64, exponent: i64) -> f64 {
    amount_minor / 10f64.powi(exponent.clamp(0, 6) as i32)
}

fn parse_extra(obj: &serde_json::Map<String, Value>) -> Option<ExtraUsage> {
    let ex = obj.get("extra_usage").filter(|e| e.is_object());
    let spend = obj.get("spend").filter(|s| s.is_object());
    if ex.is_none() && spend.is_none() {
        return None;
    }
    let exf = |k: &str| ex.and_then(|e| e.get(k)).and_then(|x| x.as_f64());
    let exb = |k: &str| ex.and_then(|e| e.get(k)).and_then(|x| x.as_bool());
    let exs = |k: &str| ex.and_then(|e| e.get(k)).and_then(|x| x.as_str()).map(str::to_string);

    // `extra_usage.used_credits` / `monthly_limit` are in minor units
    // (`decimal_places`, 2 when absent). Prefer the explicit `spend.used` money object.
    let decimals = ex
        .and_then(|e| e.get("decimal_places"))
        .and_then(|d| d.as_i64())
        .unwrap_or(2);
    let money = |v: Option<&Value>| -> Option<f64> {
        let v = v?;
        if let Some(o) = v.as_object() {
            let minor = o.get("amount_minor").and_then(|x| x.as_f64())?;
            let exp = o.get("exponent").and_then(|x| x.as_i64()).unwrap_or(decimals);
            Some(minor_to_units(minor, exp))
        } else {
            v.as_f64().map(|n| minor_to_units(n, decimals))
        }
    };

    let used_dollars = money(spend.and_then(|s| s.get("used")))
        .or_else(|| exf("used_credits").map(|n| minor_to_units(n, decimals)))
        .unwrap_or(0.0);
    let limit_dollars = exf("monthly_limit")
        .map(|n| minor_to_units(n, decimals))
        .or_else(|| money(spend.and_then(|s| s.get("limit"))))
        .unwrap_or(0.0);

    let is_enabled = exb("is_enabled")
        .or_else(|| spend.and_then(|s| s.get("enabled")).and_then(|b| b.as_bool()))
        .unwrap_or(false);
    let disabled_reason = exs("disabled_reason")
        .or_else(|| spend.and_then(|s| s.get("disabled_reason")).and_then(|x| x.as_str()).map(str::to_string))
        .unwrap_or_default();
    let currency = exs("currency")
        .or_else(|| {
            spend
                .and_then(|s| s.get("used"))
                .and_then(|u| u.get("currency"))
                .and_then(|c| c.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "USD".to_string());
    let utilization = exf("utilization")
        .map(norm_util)
        .or_else(|| spend.and_then(|s| s.get("percent")).and_then(|p| p.as_f64()).map(norm_util))
        .unwrap_or(if limit_dollars > 0.0 { (used_dollars / limit_dollars).clamp(0.0, 1.0) } else { 0.0 });

    Some(ExtraUsage {
        is_enabled,
        used_dollars,
        limit_dollars,
        currency,
        utilization,
        disabled_reason,
        spend_limit_reached: exb("spend_limit_reached").unwrap_or(false),
        can_purchase_credits: spend
            .and_then(|s| s.get("can_purchase_credits"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
    })
}

/// Parse a usage response body. Prefers the normalized `limits[]` array and
/// merges in any legacy top-level bucket it doesn't already cover.
fn parse_usage(body: &Value, notes: &[(String, String)]) -> RealUsage {
    let obj = match body.as_object() {
        Some(o) => o,
        None => return not_found("unexpected usage shape"),
    };
    let mut out = RealUsage {
        found: true,
        fetched_at: Utc::now().timestamp_millis(),
        ..Default::default()
    };

    // 1) Normalized limits[] (session + weekly windows, incl. per-model ones).
    let mut weekly: Vec<RealBucket> = Vec::new();
    if let Some(limits) = obj.get("limits").and_then(|l| l.as_array()) {
        for item in limits {
            if let Some((is_session, b)) = limit_bucket(item) {
                if is_session {
                    if out.session.is_none() {
                        out.session = Some(b);
                    }
                } else if !weekly.iter().any(|w| w.label.eq_ignore_ascii_case(&b.label)) {
                    weekly.push(b);
                }
            }
        }
    }

    // 2) Legacy top-level keys fill any gaps (and are the whole story on older responses).
    if out.session.is_none() {
        if let Some(five) = obj.get("five_hour") {
            out.session = legacy_bucket("five_hour", "Current session", five);
        }
    }
    for (key, val) in obj {
        if let Some(label) = legacy_weekly_label(key) {
            if weekly.iter().any(|w| w.label.eq_ignore_ascii_case(label)) {
                continue;
            }
            if let Some(b) = legacy_bucket(key, label, val) {
                weekly.push(b);
            }
        }
    }
    weekly.sort_by_key(|b| label_rank(&b.label));

    // 3) Promo / boost notices attached to their bar.
    for (bar, text) in notes {
        let target = note_target_label(bar);
        if let Some(s) = out.session.as_mut() {
            if s.key == *bar || target == Some("Current session") {
                s.note = text.clone();
                continue;
            }
        }
        if let Some(w) = weekly
            .iter_mut()
            .find(|w| w.key == *bar || target.map(|t| w.label.eq_ignore_ascii_case(t)).unwrap_or(false))
        {
            w.note = text.clone();
        }
    }
    out.weekly = weekly;

    // 4) Extra usage / spend (authoritative overage billing).
    out.extra = parse_extra(obj);

    out
}

fn from_cache(fetched_at_ms: i64, body: &Value, notes: &[(String, String)], reason: &str) -> RealUsage {
    let mut u = parse_usage(body, notes);
    if u.found {
        u.source = "cache".into();
        u.fetched_at = fetched_at_ms;
        u.reason = reason.to_string();
    }
    u
}

// ─── Fetch ──────────────────────────────────────────────────────────────────

pub async fn fetch(env_id: &str) -> RealUsage {
    let config_dir = match env::config_dir_for(env_id) {
        Some(d) => d,
        None => return not_found("unknown environment"),
    };
    let now_ms = Utc::now().timestamp_millis();
    let today = chrono::Local::now().date_naive();

    // Claude Code's account file: cached usage + promo notices (both optional).
    let account_json = env::account_file_for_env(env_id).and_then(|p| read_json(&p));
    let (notes, promo_cached_at) = account_json
        .as_ref()
        .map(|j| promo_notes(j, today))
        .unwrap_or_default();
    let cache = account_json
        .as_ref()
        .and_then(cached_usage)
        .filter(|(fetched, _)| cache_usable(*fetched, now_ms));

    let finish = |mut u: RealUsage| {
        u.promo_cached_at = promo_cached_at;
        u
    };

    // Fresh cache → no network call needed.
    if let Some((fetched, body)) = &cache {
        if now_ms - *fetched < CACHE_FRESH_MS {
            return finish(from_cache(*fetched, body, &notes, ""));
        }
    }

    // Any failure below falls back to the (somewhat stale, < 1h) cache when we have one.
    let fallback = |reason: &str, status: u16, retry_after: u64| -> RealUsage {
        let mut u = match &cache {
            Some((fetched, body)) => from_cache(*fetched, body, &notes, reason),
            None => not_found(reason),
        };
        u.http_status = status;
        u.retry_after_secs = retry_after;
        u
    };

    let token = match load_valid_token(&config_dir) {
        Ok(t) => t,
        Err(e) => return finish(fallback(&e, 0, 0)),
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return finish(fallback("http client error", 0, 0)),
    };

    let resp = client
        .get(USAGE_URL)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", "claude-usage-widget/0.2")
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(_) => return finish(fallback("request failed", 0, 0)),
    };
    let status = resp.status();
    if !status.is_success() {
        // Status only — never the token or auth headers.
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        return finish(fallback(&format!("usage api returned {}", status.as_u16()), status.as_u16(), retry_after));
    }
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return finish(fallback("bad usage response", status.as_u16(), 0)),
    };

    let mut u = parse_usage(&body, &notes);
    u.http_status = status.as_u16();
    if u.found {
        u.source = "api".into();
    }
    finish(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// Trimmed copy of a real response cached by Claude Code 2.1.258.
    fn modern_body() -> Value {
        serde_json::from_str(
            r#"{
              "five_hour": {"utilization": 1, "resets_at": "2026-09-02T13:49:59.826640+00:00", "limit_dollars": null, "locked_reason": null},
              "seven_day": {"utilization": 10, "resets_at": "2026-09-04T20:59:59.826665+00:00"},
              "seven_day_oauth_apps": null, "seven_day_opus": null, "seven_day_sonnet": null,
              "nimbus_quill": {"utilization": 0, "resets_at": null},
              "cinder_cove": null,
              "extra_usage": {"is_enabled": true, "monthly_limit": null, "used_credits": 5128, "utilization": null, "currency": "USD", "decimal_places": 2, "disabled_reason": null, "spend_limit_reached": false},
              "limits": [
                {"kind": "session", "group": "session", "percent": 1, "severity": "normal", "resets_at": "2026-09-02T13:49:59.826640+00:00", "scope": null, "is_active": false},
                {"kind": "weekly_all", "group": "weekly", "percent": 10, "severity": "normal", "resets_at": "2026-09-04T20:59:59.826665+00:00", "scope": null, "is_active": false},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 14, "severity": "normal", "resets_at": "2026-09-04T20:59:59.826901+00:00", "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}, "is_active": true}
              ],
              "spend": {"used": {"amount_minor": 5128, "currency": "USD", "exponent": 2}, "limit": null, "percent": 0, "severity": "normal", "enabled": true, "disabled_reason": null, "can_purchase_credits": false, "can_toggle": false}
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn normalizes_utilization_to_unit_range() {
        assert!(close(norm_util(38.0), 0.38));
        assert!(close(norm_util(1.0), 0.01)); // 1 means 1%, not 100%
        assert_eq!(norm_util(250.0), 1.0); // clamped
        assert_eq!(norm_util(-3.0), 0.0);
    }

    #[test]
    fn parses_modern_limits_array_and_spend() {
        let u = parse_usage(&modern_body(), &[]);
        assert!(u.found);

        let s = u.session.as_ref().expect("session");
        assert!(close(s.utilization, 0.01));
        assert_eq!(s.resets_at, "2026-09-02T13:49:59.826640+00:00");

        // Fable comes from limits[] (weekly_scoped) — legacy keys never carried it.
        let labels: Vec<&str> = u.weekly.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["All models", "Fable only"]);
        let fable = &u.weekly[1];
        assert!(close(fable.utilization, 0.14));
        assert!(fable.is_active);
        assert_eq!(fable.key, "model:fable");
        assert_eq!(fable.severity, "normal");

        // Extra usage is in minor units: 5128 → $51.28, no cap.
        let ex = u.extra.expect("extra usage");
        assert!(ex.is_enabled);
        assert!(close(ex.used_dollars, 51.28), "used {}", ex.used_dollars);
        assert_eq!(ex.limit_dollars, 0.0);
        assert_eq!(ex.currency, "USD");
        assert!(!ex.spend_limit_reached);
    }

    #[test]
    fn legacy_body_still_parses_and_used_credits_are_minor_units() {
        let body: Value = serde_json::from_str(
            r#"{
              "five_hour": {"utilization": 38, "resets_at": "2026-05-26T20:00:00Z"},
              "seven_day": {"utilization": 10, "resets_at": "2026-05-29T14:00:00Z"},
              "seven_day_sonnet": {"utilization": 2, "resets_at": "2026-05-29T14:00:00Z"},
              "seven_day_omelette": {"utilization": 2, "resets_at": "2026-05-29T14:00:00Z"},
              "seven_day_overage_included": {"utilization": 20, "resets_at": "2026-05-29T14:00:00Z"},
              "seven_day_cowork": {"utilization": 0, "resets_at": "x"},
              "extra_usage": {"is_enabled": true, "used_credits": 806, "monthly_limit": 100000, "currency": "USD", "utilization": 80}
            }"#,
        )
        .unwrap();
        let u = parse_usage(&body, &[]);

        assert!(u.found);
        assert!(close(u.session.as_ref().unwrap().utilization, 0.38));

        // Allowlisted, ranked; overage_included → Fable; omelette → Claude Design; cowork excluded.
        let labels: Vec<&str> = u.weekly.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["All models", "Fable only", "Sonnet only", "Claude Design"]);

        let ex = u.extra.expect("extra usage parsed");
        assert!(close(ex.used_dollars, 8.06));
        assert!(close(ex.limit_dollars, 1000.0));
        assert!(close(ex.utilization, 0.8));
    }

    #[test]
    fn limits_and_legacy_are_merged_without_duplicates() {
        // limits[] lacks weekly_all; legacy seven_day fills it in exactly once.
        let body = serde_json::json!({
            "seven_day": {"utilization": 30, "resets_at": 1788000000},
            "limits": [
                {"kind": "weekly_scoped", "percent": null, "scope": {"model": {"display_name": "Opus"}}},
                {"kind": "weekly_scoped", "percent": 55, "resets_at": 1788000000, "scope": {"model": {"display_name": "Fable"}}}
            ]
        });
        let u = parse_usage(&body, &[]);
        let labels: Vec<&str> = u.weekly.iter().map(|b| b.label.as_str()).collect();
        // Opus skipped (null percent); numeric resets_at normalized to ISO.
        assert_eq!(labels, vec!["All models", "Fable only"]);
        let expected = chrono::DateTime::from_timestamp(1788000000, 0).unwrap().to_rfc3339();
        assert_eq!(u.weekly[0].resets_at, expected);
        assert_eq!(u.weekly[1].resets_at, expected);
        assert!(u.session.is_none());
    }

    #[test]
    fn promo_notes_attach_to_their_bar() {
        let notes = vec![
            ("seven_day".to_string(), "+50% weekly limits promo through Sep 13 · clau.de/cc-50-promo".to_string()),
            ("five_hour".to_string(), "session boost".to_string()),
        ];
        let u = parse_usage(&modern_body(), &notes);
        assert_eq!(u.session.unwrap().note, "session boost");
        assert_eq!(u.weekly[0].note, "+50% weekly limits promo through Sep 13 · clau.de/cc-50-promo");
        assert_eq!(u.weekly[1].note, "");
    }

    #[test]
    fn promo_expiry_parses_the_through_date() {
        let today = d(2026, 9, 2);
        assert!(!promo_expired("+50% weekly limits promo through Sep 13 · clau.de/x", today));
        assert!(promo_expired("+50% weekly limits promo through Aug 31 · clau.de/x", today));
        assert!(!promo_expired("+50% weekly limits promo through Sep 2", today)); // inclusive
        assert!(!promo_expired("limits boosted this week", today)); // unparseable → keep
        // January promo read in December belongs to next year.
        assert!(!promo_expired("promo through Jan 5", d(2026, 12, 20)));
    }

    #[test]
    fn reads_claude_code_cache_and_notices_from_account_json() {
        let account = serde_json::json!({
            "cachedUsageUtilization": {"fetchedAtMs": 1788380000000i64, "accountUuid": "x", "utilization": modern_body()},
            "cachedGrowthBookFeaturesAt": 1788390000000i64,
            "cachedGrowthBookFeatures": {
                "tengu_rate_limit_promo_notices": [
                    {"bar": "seven_day", "text": "+50% weekly limits promo through Sep 13", "variant": "claude"},
                    {"bar": "seven_day", "text": "old promo through Aug 31"}
                ]
            }
        });
        let (fetched, body) = cached_usage(&account).expect("cache");
        assert_eq!(fetched, 1788380000000);
        let (notes, cached_at) = promo_notes(&account, d(2026, 9, 2));
        assert_eq!(cached_at, 1788390000000);
        assert_eq!(notes.len(), 1, "expired notice dropped");

        let u = from_cache(fetched, &body, &notes, "usage api returned 429");
        assert!(u.found);
        assert_eq!(u.source, "cache");
        assert_eq!(u.fetched_at, 1788380000000);
        assert_eq!(u.reason, "usage api returned 429");
        assert_eq!(u.weekly[0].note, "+50% weekly limits promo through Sep 13");
    }

    #[test]
    fn cache_is_usable_for_one_hour_only() {
        let now = 1_788_400_000_000i64;
        assert!(cache_usable(now - 4 * 60_000, now)); // 4 min old
        assert!(cache_usable(now - 59 * 60_000, now)); // 59 min old
        assert!(!cache_usable(now - 61 * 60_000, now)); // over an hour → drop
        assert!(!cache_usable(now + 60_000, now)); // clock skew into the future → drop
    }

    #[test]
    fn non_object_body_is_not_found() {
        assert!(!parse_usage(&serde_json::json!([1, 2, 3]), &[]).found);
    }

    /// Manual smoke test against a real environment (hits the network and reads
    /// the real account file). Run with:
    /// `CLAUDE_USAGE_SMOKE_ENV=.claude-team cargo test -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn smoke_fetch_real_env() {
        let env_id = std::env::var("CLAUDE_USAGE_SMOKE_ENV").unwrap_or_else(|_| ".claude".into());
        let u = tauri::async_runtime::block_on(fetch(&env_id));
        println!("{}", serde_json::to_string_pretty(&u).unwrap());
        let d = crate::usage::collect_at(env::projects_dir_for(&env_id), None);
        println!("local: is_mock={} today.cost=${:.2} today.prompts={} models={}", d.is_mock, d.today.cost, d.today.prompts,
            d.models.iter().map(|m| format!("{}:{}tok/${:.2}", m.id, m.tokens, m.cost)).collect::<Vec<_>>().join(" "));
    }
}
