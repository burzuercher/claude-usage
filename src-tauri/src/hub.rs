//! Shared, in-memory copy of the latest data, so every consumer — the widget
//! window and the iTerm2 panel server — reads from one poller.
//!
//! This matters for the live limits: `/api/oauth/usage` allows one request per
//! token per 2-minute window, shared with Claude Code itself. A second
//! independent poller would burn that budget and turn half of all requests
//! into 429s. Instead the widget keeps polling as before, every result is
//! remembered here, and a recent successful API result is handed back rather
//! than spending another request.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::realusage::RealUsage;
use crate::usage::UsageData;

/// A successful API fetch younger than this is reused instead of refetched
/// (just under the endpoint's 2-minute window, so the widget's 125s poll still
/// fetches for real).
const API_REUSE_MS: i64 = 110_000;

struct RealMemo {
    /// ms epoch of the last fetch attempt, whatever it returned.
    attempted_at: i64,
    /// Latest found result, or the latest not-found one when nothing was ever found.
    value: RealUsage,
}

struct UsageMemo {
    env: String,
    computed_at: i64,
    value: UsageData,
}

#[derive(Default)]
pub struct Hub {
    /// Environment the widget last asked about — what the panel shows.
    env: Mutex<String>,
    real: Mutex<HashMap<String, RealMemo>>,
    usage: Mutex<Option<UsageMemo>>,
    /// Serializes the panel's own refreshes so concurrent requests fetch once.
    pub refresh_lock: Mutex<()>,
}

impl Hub {
    pub fn env(&self) -> String {
        self.env.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn set_env(&self, env_id: &str) {
        if let Ok(mut e) = self.env.lock() {
            if *e != env_id {
                *e = env_id.to_string();
            }
        }
    }

    /// A recent successful API result for `env_id`, if one can stand in for a new request.
    pub fn reusable_api(&self, env_id: &str, now_ms: i64) -> Option<RealUsage> {
        let map = self.real.lock().ok()?;
        let m = map.get(env_id)?;
        let v = &m.value;
        (v.found && v.source == "api" && now_ms - v.fetched_at < API_REUSE_MS).then(|| v.clone())
    }

    /// Latest result for `env_id` and when it was last attempted.
    pub fn real(&self, env_id: &str) -> Option<(i64, RealUsage)> {
        let map = self.real.lock().ok()?;
        map.get(env_id).map(|m| (m.attempted_at, m.value.clone()))
    }

    /// Record a fetch result. A not-found result never replaces a found one, so
    /// a network blip doesn't blank the panel (the same stickiness the widget has).
    pub fn store_real(&self, env_id: &str, value: &RealUsage, now_ms: i64) {
        let Ok(mut map) = self.real.lock() else { return };
        match map.get_mut(env_id) {
            Some(m) => {
                m.attempted_at = now_ms;
                if value.found || !m.value.found {
                    m.value = value.clone();
                }
            }
            None => {
                map.insert(env_id.to_string(), RealMemo { attempted_at: now_ms, value: value.clone() });
            }
        }
    }

    /// Latest local aggregation for `env_id` and when it was computed.
    pub fn usage(&self, env_id: &str) -> Option<(i64, UsageData)> {
        let m = self.usage.lock().ok()?;
        m.as_ref().filter(|m| m.env == env_id).map(|m| (m.computed_at, m.value.clone()))
    }

    pub fn store_usage(&self, env_id: &str, value: &UsageData, now_ms: i64) {
        if let Ok(mut m) = self.usage.lock() {
            *m = Some(UsageMemo { env: env_id.to_string(), computed_at: now_ms, value: value.clone() });
        }
    }
}

/// Live window starts implied by a usage result: `(session, week)` as ms epochs
/// — the session reset − 5h and the "All models" weekly reset − 7d.
pub fn window_starts(real: &RealUsage) -> (Option<i64>, Option<i64>) {
    let start = |iso: &str, span_ms: i64| {
        chrono::DateTime::parse_from_rfc3339(iso)
            .ok()
            .map(|t| t.timestamp_millis() - span_ms)
    };
    if !real.found {
        return (None, None);
    }
    let session = real.session.as_ref().and_then(|s| start(&s.resets_at, 5 * 3600 * 1000));
    let week = real
        .weekly
        .iter()
        .find(|b| b.key == "seven_day")
        .and_then(|b| start(&b.resets_at, 7 * 24 * 3600 * 1000));
    (session, week)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(fetched_at: i64) -> RealUsage {
        RealUsage { found: true, source: "api".into(), fetched_at, ..Default::default() }
    }

    #[test]
    fn reuses_only_a_recent_api_result() {
        let hub = Hub::default();
        hub.store_real("e", &api(1_000), 1_000);
        assert!(hub.reusable_api("e", 1_000 + API_REUSE_MS - 1).is_some());
        assert!(hub.reusable_api("e", 1_000 + API_REUSE_MS).is_none());
        assert!(hub.reusable_api("other", 1_000).is_none());

        // Claude Code's cache is free to read, so it is never reused in place of a fetch.
        let mut cached = api(5_000);
        cached.source = "cache".into();
        hub.store_real("c", &cached, 5_000);
        assert!(hub.reusable_api("c", 5_001).is_none());
    }

    #[test]
    fn not_found_never_replaces_found() {
        let hub = Hub::default();
        hub.store_real("e", &api(1_000), 1_000);
        hub.store_real("e", &RealUsage { reason: "request failed".into(), ..Default::default() }, 9_000);
        let (attempted, v) = hub.real("e").unwrap();
        assert_eq!(attempted, 9_000);
        assert!(v.found);
    }

    #[test]
    fn window_starts_from_resets() {
        use crate::realusage::RealBucket;
        let bucket = |key: &str, resets_at: &str| RealBucket {
            key: key.into(),
            resets_at: resets_at.into(),
            ..Default::default()
        };
        let real = RealUsage {
            found: true,
            session: Some(bucket("five_hour", "2026-09-23T20:00:00Z")),
            weekly: vec![bucket("model:fable", "2026-09-25T00:00:00Z"), bucket("seven_day", "2026-09-26T18:00:00Z")],
            ..Default::default()
        };
        let (s, w) = window_starts(&real);
        let ms = |iso: &str| chrono::DateTime::parse_from_rfc3339(iso).unwrap().timestamp_millis();
        assert_eq!(s, Some(ms("2026-09-23T15:00:00Z")));
        assert_eq!(w, Some(ms("2026-09-19T18:00:00Z")));
    }
}
