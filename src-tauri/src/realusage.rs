//! Fetches *authoritative* usage limits from Anthropic, the same data the Claude
//! app shows under "Your usage limits" — `GET /api/oauth/usage`.
//!
//! The OAuth access token is read at runtime from the selected environment's
//! `.credentials.json` (the file Claude Code maintains). We only use the token
//! if it's still valid; we never refresh/rotate it (that could disrupt Claude
//! Code's own login) and never log or return it.

use serde::Serialize;
use serde_json::Value;
use std::path::Path;

use crate::env;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RealBucket {
    pub key: String,
    pub label: String,
    pub utilization: f64, // 0..1
    pub resets_at: String, // ISO timestamp, may be empty
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtraUsage {
    pub is_enabled: bool,
    pub used_credits: f64, // dollars spent on extra usage this month
    pub monthly_limit: f64, // dollar cap (0 = none)
    pub currency: String,
    pub utilization: f64, // 0..1 of the extra-usage budget
    pub disabled_reason: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RealUsage {
    pub found: bool,
    pub reason: String,
    pub session: Option<RealBucket>,
    pub weekly: Vec<RealBucket>,
    pub extra: Option<ExtraUsage>,
    pub fetched_at: i64,
}

fn not_found(reason: &str) -> RealUsage {
    RealUsage {
        found: false,
        reason: reason.to_string(),
        fetched_at: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    }
}

/// Read `claudeAiOauth.accessToken` + `expiresAt` from an env's credentials file.
/// Returns None (with a reason) if missing or expired.
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
        if exp <= chrono::Utc::now().timestamp_millis() {
            return Err("token expired — run Claude Code to refresh".into());
        }
    }
    Ok(token)
}

/// Weekly buckets we surface, with display label and order. Anthropic returns
/// extra internal buckets (cowork, oauth_apps, codenames) that the Claude app
/// doesn't show, so we allowlist the user-facing ones. "omelette" is Anthropic's
/// internal codename for Claude Design.
fn weekly_meta(key: &str) -> Option<(&'static str, u8)> {
    match key {
        "seven_day" => Some(("All models", 0)),
        "seven_day_opus" => Some(("Opus only", 1)),
        "seven_day_sonnet" => Some(("Sonnet only", 2)),
        "seven_day_omelette" => Some(("Claude Design", 3)),
        _ => None,
    }
}

/// utilization may come back as 0..1 or 0..100; normalize to 0..1.
fn norm_util(v: f64) -> f64 {
    let u = if v > 1.0 { v / 100.0 } else { v };
    u.clamp(0.0, 1.0)
}

fn bucket_from(key: &str, label: &str, obj: &Value) -> Option<RealBucket> {
    let util = obj.get("utilization").and_then(|u| u.as_f64())?;
    let resets_at = obj
        .get("resets_at")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    Some(RealBucket {
        key: key.to_string(),
        label: label.to_string(),
        utilization: norm_util(util),
        resets_at,
    })
}

pub async fn fetch(env_id: &str) -> RealUsage {
    let config_dir = match env::config_dir_for(env_id) {
        Some(d) => d,
        None => return not_found("unknown environment"),
    };
    let token = match load_valid_token(&config_dir) {
        Ok(t) => t,
        Err(e) => return not_found(&e),
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return not_found("http client error"),
    };

    let resp = client
        .get(USAGE_URL)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("User-Agent", "claude-usage-widget/0.1")
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(_) => return not_found("request failed"),
    };
    let status = resp.status();
    if !status.is_success() {
        // Status only — never the token or auth headers.
        return not_found(&format!("usage api returned {}", status.as_u16()));
    }
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return not_found("bad usage response"),
    };

    parse_usage(body)
}

fn parse_usage(body: Value) -> RealUsage {
    let mut out = RealUsage {
        found: true,
        fetched_at: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    };

    let obj = match body.as_object() {
        Some(o) => o,
        None => return not_found("unexpected usage shape"),
    };

    // Session (current 5-hour window).
    if let Some(five) = obj.get("five_hour") {
        out.session = bucket_from("five_hour", "Current session", five);
    }

    // Weekly buckets (allowlisted, in display order).
    let mut weekly: Vec<(u8, RealBucket)> = Vec::new();
    for (key, val) in obj {
        if let Some((label, order)) = weekly_meta(key) {
            if let Some(b) = bucket_from(key, label, val) {
                weekly.push((order, b));
            }
        }
    }
    weekly.sort_by_key(|(o, _)| *o);
    out.weekly = weekly.into_iter().map(|(_, b)| b).collect();

    // Extra usage (authoritative overage billing).
    if let Some(ex) = obj.get("extra_usage").and_then(|e| e.as_object()) {
        let f = |k: &str| ex.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
        out.extra = Some(ExtraUsage {
            is_enabled: ex.get("is_enabled").and_then(|x| x.as_bool()).unwrap_or(false),
            used_credits: f("used_credits"),
            monthly_limit: f("monthly_limit"),
            currency: ex.get("currency").and_then(|x| x.as_str()).unwrap_or("USD").to_string(),
            utilization: norm_util(f("utilization")),
            disabled_reason: ex.get("disabled_reason").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        });
    }

    out
}
