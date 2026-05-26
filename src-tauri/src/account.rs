//! Reads the currently-active Claude account from `~/.claude.json`.
//!
//! Claude Code stores the logged-in account under `oauthAccount`. This lets the
//! widget auto-detect your plan and follow whichever account you're logged into
//! (switching accounts in Claude Code updates this file, and the widget polls).
//!
//! Note: only the *active* account is stored, and usage logs are not tagged with
//! an account, so historical usage cannot be split across accounts.

use serde::Serialize;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub found: bool,
    pub email: String,
    pub org_name: String,
    pub org_type: String,            // e.g. "claude_team"
    pub seat_tier: String,           // e.g. "team_tier_1"
    pub user_rate_limit_tier: String, // e.g. "default_claude_max_5x"
    /// Mapped plan tier id: "pro" | "max5" | "max20" | "team".
    pub detected_plan: String,
}

/// Map Anthropic's tier strings to our plan ids.
fn detect_plan(org_type: &str, user_tier: &str) -> &'static str {
    let ot = org_type.to_ascii_lowercase();
    if ot.contains("team") || ot.contains("enterprise") {
        return "team";
    }
    let ut = user_tier.to_ascii_lowercase();
    if ut.contains("max_20x") || ut.contains("max20") {
        "max20"
    } else if ut.contains("max_5x") || ut.contains("max5") || ut.contains("max") {
        "max5"
    } else if ut.contains("pro") {
        "pro"
    } else {
        "max5"
    }
}

/// Read account info from a specific `.claude.json` path.
pub fn read_account_at(path: &std::path::Path) -> Account {
    if !path.exists() {
        return Account::default();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Account::default(),
    };
    let v: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Account::default(),
    };
    let oa = match v.get("oauthAccount") {
        Some(o) => o,
        None => return Account::default(),
    };
    let get = |k: &str| oa.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();

    let org_type = get("organizationType");
    let user_rate_limit_tier = get("userRateLimitTier");
    let detected_plan = detect_plan(&org_type, &user_rate_limit_tier).to_string();

    Account {
        found: true,
        email: get("emailAddress"),
        org_name: get("organizationName"),
        org_type,
        seat_tier: get("seatTier"),
        user_rate_limit_tier,
        detected_plan,
    }
}
