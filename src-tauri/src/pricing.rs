//! Per-model token pricing, used to estimate spend from local usage logs.
//!
//! Rates are USD per **million** tokens and reflect Anthropic's published
//! first-party list prices as of 2026-09 (Claude 5 family + still-served 4.x).
//! They are intentionally easy to update — adjust `price_for_model` below.
//!
//! Prompt-cache pricing: a 5-minute cache write costs 1.25× input, a 1-hour
//! write costs 2× input, and a cache read is 0.1× input — except Fable 5.1 /
//! Mythos 5.1, whose cache reads are 0.025× ($0.25/MTok).

/// A model's price card (USD per 1M tokens).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    /// 5-minute ephemeral cache write.
    pub cache_write: f64,
    /// 1-hour ephemeral cache write.
    pub cache_write_1h: f64,
    pub cache_read: f64,
}

const fn card(input: f64, output: f64, cache_write: f64, cache_write_1h: f64, cache_read: f64) -> Price {
    Price { input, output, cache_write, cache_write_1h, cache_read }
}

/// Fable 5.1 / Mythos 5.1 — $10 / $50, cache reads $0.25.
const FABLE_51: Price = card(10.0, 50.0, 12.50, 20.0, 0.25);
/// Fable 5 / Mythos 5 — same tier, cache reads at the usual 0.1× ($1.00).
const FABLE_5: Price = card(10.0, 50.0, 12.50, 20.0, 1.00);
/// Opus 4.5 → 5 — $5 / $25.
const OPUS: Price = card(5.0, 25.0, 6.25, 10.0, 0.50);
/// Opus 4 / 4.1 (legacy) — $15 / $75.
const OPUS_LEGACY: Price = card(15.0, 75.0, 18.75, 30.0, 1.50);
/// Sonnet 5 — $2 / $10.
const SONNET_5: Price = card(2.0, 10.0, 2.50, 4.0, 0.20);
/// Sonnet 4.x / 3.7 — $3 / $15.
const SONNET: Price = card(3.0, 15.0, 3.75, 6.0, 0.30);
/// Haiku 4.5 — $1 / $5.
const HAIKU: Price = card(1.0, 5.0, 1.25, 2.0, 0.10);

/// Buckets a raw model id (e.g. "claude-fable-5-1") into a family.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Family {
    Fable,
    Opus,
    Sonnet,
    Haiku,
    Other,
}

impl Family {
    /// Same bucketing Claude Code uses: substring match, Fable/Mythos first.
    pub fn from_model(model: &str) -> Family {
        let m = model.to_ascii_lowercase();
        if m.contains("fable") || m.contains("mythos") {
            Family::Fable
        } else if m.contains("opus") {
            Family::Opus
        } else if m.contains("sonnet") {
            Family::Sonnet
        } else if m.contains("haiku") {
            Family::Haiku
        } else {
            Family::Other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Family::Fable => "Fable",
            Family::Opus => "Opus",
            Family::Sonnet => "Sonnet",
            Family::Haiku => "Haiku",
            Family::Other => "Other",
        }
    }

    /// Stable id used by the frontend (`MODEL_COLOR`, ordering).
    pub fn id(self) -> &'static str {
        match self {
            Family::Fable => "fable",
            Family::Opus => "opus",
            Family::Sonnet => "sonnet",
            Family::Haiku => "haiku",
            Family::Other => "other",
        }
    }

    /// Display order: newest/most expensive tier first.
    pub fn order(self) -> u8 {
        match self {
            Family::Fable => 0,
            Family::Opus => 1,
            Family::Sonnet => 2,
            Family::Haiku => 3,
            Family::Other => 4,
        }
    }
}

/// Version-aware price lookup for a raw model id.
pub fn price_for_model(model: &str) -> Price {
    let m = model.to_ascii_lowercase();
    match Family::from_model(&m) {
        Family::Fable => {
            if m.contains("fable-5-1") || m.contains("mythos-5-1") {
                FABLE_51
            } else {
                FABLE_5
            }
        }
        Family::Opus => {
            // "claude-opus-4-1[-date]" / "claude-opus-4-0" / "claude-opus-4-2025…"
            if m.contains("opus-4-1") || m.contains("opus-4-0") || m.contains("opus-4-2025") {
                OPUS_LEGACY
            } else {
                OPUS
            }
        }
        Family::Sonnet => {
            if m.contains("sonnet-5") {
                SONNET_5
            } else {
                SONNET
            }
        }
        Family::Haiku => HAIKU,
        // Unknown — mid-tier guess so cost isn't zero.
        Family::Other => SONNET_5,
    }
}

/// Cost in USD for a single turn's token counts.
pub fn turn_cost(
    model: &str,
    input: u64,
    output: u64,
    cache_write_5m: u64,
    cache_write_1h: u64,
    cache_read: u64,
) -> f64 {
    let p = price_for_model(model);
    let per = 1_000_000.0;
    (input as f64) * p.input / per
        + (output as f64) * p.output / per
        + (cache_write_5m as f64) * p.cache_write / per
        + (cache_write_1h as f64) * p.cache_write_1h / per
        + (cache_read as f64) * p.cache_read / per
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn buckets_model_ids_into_families() {
        assert_eq!(Family::from_model("claude-fable-5-1"), Family::Fable);
        assert_eq!(Family::from_model("claude-mythos-5-1"), Family::Fable);
        assert_eq!(Family::from_model("claude-opus-5"), Family::Opus);
        assert_eq!(Family::from_model("claude-opus-4-7"), Family::Opus);
        assert_eq!(Family::from_model("claude-sonnet-5"), Family::Sonnet);
        assert_eq!(Family::from_model("claude-haiku-4-5-20251001"), Family::Haiku);
        assert_eq!(Family::from_model("some-other-model"), Family::Other);
    }

    #[test]
    fn fable_rates_and_cache_read_generations() {
        // 1M in @ $10 + 1M out @ $50 = $60
        assert!(close(turn_cost("claude-fable-5-1", 1_000_000, 1_000_000, 0, 0, 0), 60.0));
        // Fable 5.1 cache read is $0.25/MTok; Fable 5 is $1.00/MTok.
        assert!(close(turn_cost("claude-fable-5-1", 0, 0, 0, 0, 1_000_000), 0.25));
        assert!(close(turn_cost("claude-fable-5", 0, 0, 0, 0, 1_000_000), 1.00));
        assert_eq!(price_for_model("claude-mythos-5-1"), price_for_model("claude-fable-5-1"));
    }

    #[test]
    fn opus_uses_5_25_rates_and_legacy_15_75() {
        assert!(close(turn_cost("claude-opus-5", 1_000_000, 1_000_000, 0, 0, 0), 30.0));
        assert!(close(turn_cost("claude-opus-4-7", 0, 0, 0, 0, 1_000_000), 0.50));
        assert!(close(turn_cost("claude-opus-4-1-20250805", 1_000_000, 1_000_000, 0, 0, 0), 90.0));
    }

    #[test]
    fn sonnet_5_is_cheaper_than_sonnet_4() {
        assert!(close(turn_cost("claude-sonnet-5", 1_000_000, 1_000_000, 0, 0, 0), 12.0));
        assert!(close(turn_cost("claude-sonnet-4-6", 1_000_000, 1_000_000, 0, 0, 0), 18.0));
    }

    #[test]
    fn one_hour_cache_write_costs_double_input() {
        // Opus: 5m write = $6.25, 1h write = $10.00 per MTok.
        assert!(close(turn_cost("claude-opus-5", 0, 0, 1_000_000, 0, 0), 6.25));
        assert!(close(turn_cost("claude-opus-5", 0, 0, 0, 1_000_000, 0), 10.0));
    }

    #[test]
    fn zero_tokens_zero_cost() {
        assert_eq!(turn_cost("claude-sonnet-5", 0, 0, 0, 0, 0), 0.0);
    }
}
