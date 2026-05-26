//! Per-model token pricing, used to estimate spend from local usage logs.
//!
//! Rates are USD per **million** tokens and reflect Anthropic's published
//! list prices for the Claude 4.x family. They are intentionally easy to
//! update — adjust the constants below if pricing changes.

/// A model family's price card (USD per 1M tokens).
#[derive(Clone, Copy)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    /// 5-minute ephemeral cache write.
    pub cache_write: f64,
    pub cache_read: f64,
}

/// Buckets a raw model id (e.g. "claude-opus-4-7") into a family.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Family {
    Opus,
    Sonnet,
    Haiku,
    Other,
}

impl Family {
    pub fn from_model(model: &str) -> Family {
        let m = model.to_ascii_lowercase();
        if m.contains("opus") {
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
            Family::Opus => "Opus",
            Family::Sonnet => "Sonnet",
            Family::Haiku => "Haiku",
            Family::Other => "Other",
        }
    }

    pub fn price(self) -> Price {
        match self {
            // Opus 4.7 — $5 / $25 per Mtok (verified against ccusage/LiteLLM).
            Family::Opus => Price { input: 5.0, output: 25.0, cache_write: 6.25, cache_read: 0.50 },
            // Sonnet 4.x
            Family::Sonnet => Price { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 },
            // Haiku 4.5
            Family::Haiku => Price { input: 1.0, output: 5.0, cache_write: 1.25, cache_read: 0.10 },
            // Unknown — approximate with Sonnet rates so cost isn't zero.
            Family::Other => Price { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 },
        }
    }
}

/// Cost in USD for a single turn's token counts.
pub fn turn_cost(
    family: Family,
    input: u64,
    output: u64,
    cache_write: u64,
    cache_read: u64,
) -> f64 {
    let p = family.price();
    let per = 1_000_000.0;
    (input as f64) * p.input / per
        + (output as f64) * p.output / per
        + (cache_write as f64) * p.cache_write / per
        + (cache_read as f64) * p.cache_read / per
}
