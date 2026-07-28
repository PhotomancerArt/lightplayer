//! Estimated-cost math for the chat usage footnote. Pure: a small dated
//! pricing table plus formatting; settings rate overrides win over the
//! table, and an unknown model with no override yields no estimate.

use crate::app::agent::ui_agent_view::UiAgentUsage;

/// Known model prices by id prefix — `(prefix, $/MTok input, $/MTok
/// output)`. Prices as of 2026-07. Prefix matching keeps dated ids
/// (`claude-sonnet-5-20260203`-style) matched to their family.
const MODEL_PRICES: &[(&str, f64, f64)] = &[
    ("claude-sonnet-5", 3.0, 15.0),
    ("claude-opus-4-8", 5.0, 25.0),
    ("claude-opus-4-7", 5.0, 25.0),
    ("claude-haiku-4-5", 1.0, 5.0),
];

/// Prompt-cache write premium over the input rate (Anthropic ephemeral
/// cache, default 5-minute TTL; as of 2026-07). Settings overrides carry
/// only input/output rates, so cache rates always derive from the input
/// rate by these multipliers.
pub const CACHE_WRITE_INPUT_MULTIPLIER: f64 = 1.25;
/// Prompt-cache read discount off the input rate (as of 2026-07).
pub const CACHE_READ_INPUT_MULTIPLIER: f64 = 0.1;

/// Cost rates for one model, $ per million tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgentCostRates {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    /// Cache-write rate ([`CACHE_WRITE_INPUT_MULTIPLIER`] × input).
    pub cache_write_per_mtok: f64,
    /// Cache-read rate ([`CACHE_READ_INPUT_MULTIPLIER`] × input).
    pub cache_read_per_mtok: f64,
}

impl AgentCostRates {
    /// Rates from an input/output pair; the cache rates derive from the
    /// input rate by the standard multipliers.
    pub fn from_io(input_per_mtok: f64, output_per_mtok: f64) -> Self {
        Self {
            input_per_mtok,
            output_per_mtok,
            cache_write_per_mtok: input_per_mtok * CACHE_WRITE_INPUT_MULTIPLIER,
            cache_read_per_mtok: input_per_mtok * CACHE_READ_INPUT_MULTIPLIER,
        }
    }

    /// Table rates for a model id (longest configured prefix wins), or
    /// `None` for unknown models.
    pub fn for_model(model: &str) -> Option<Self> {
        MODEL_PRICES
            .iter()
            .filter(|(prefix, _, _)| model.starts_with(prefix))
            .max_by_key(|(prefix, _, _)| prefix.len())
            .map(|&(_, input, output)| Self::from_io(input, output))
    }

    /// The estimated cost in dollars for a session's usage: the four
    /// disjoint buckets (uncached input, output, cache writes, cache
    /// reads) each priced at their own rate.
    pub fn estimate_usd(&self, usage: UiAgentUsage) -> f64 {
        (f64::from(usage.input_tokens) * self.input_per_mtok
            + f64::from(usage.output_tokens) * self.output_per_mtok
            + f64::from(usage.cache_write_tokens) * self.cache_write_per_mtok
            + f64::from(usage.cache_read_tokens) * self.cache_read_per_mtok)
            / 1_000_000.0
    }
}

/// Format a dollar estimate for the footnote: 4 decimal places under a
/// cent, 2 above (`$0.0042`, `$1.23`).
pub fn format_cost_usd(usd: f64) -> String {
    if usd < 0.01 {
        format!("${usd:.4}")
    } else {
        format!("${usd:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u32, output: u32, cache_write: u32, cache_read: u32) -> UiAgentUsage {
        UiAgentUsage {
            input_tokens: input,
            output_tokens: output,
            cache_write_tokens: cache_write,
            cache_read_tokens: cache_read,
        }
    }

    #[test]
    fn table_matches_by_prefix() {
        let rates = AgentCostRates::for_model("claude-sonnet-5").expect("known");
        assert_eq!(rates.input_per_mtok, 3.0);
        assert_eq!(rates.output_per_mtok, 15.0);
        // Dated ids match their family prefix.
        let dated = AgentCostRates::for_model("claude-haiku-4-5-20260101").expect("known");
        assert_eq!(dated.input_per_mtok, 1.0);
    }

    #[test]
    fn cache_rates_derive_from_the_input_rate() {
        let rates = AgentCostRates::for_model("claude-sonnet-5").expect("known");
        assert!((rates.cache_write_per_mtok - 3.75).abs() < 1e-12);
        assert!((rates.cache_read_per_mtok - 0.3).abs() < 1e-12);
        // The same multipliers apply to arbitrary (override) input rates.
        let overridden = AgentCostRates::from_io(2.0, 6.0);
        assert!((overridden.cache_write_per_mtok - 2.5).abs() < 1e-12);
        assert!((overridden.cache_read_per_mtok - 0.2).abs() < 1e-12);
    }

    #[test]
    fn unknown_model_has_no_rates() {
        assert_eq!(AgentCostRates::for_model("gpt-5.2-mini"), None);
        assert_eq!(AgentCostRates::for_model(""), None);
    }

    #[test]
    fn estimate_math_is_per_million_tokens() {
        let rates = AgentCostRates::from_io(3.0, 15.0);
        // 1M in + 1M out = $18.
        assert!((rates.estimate_usd(usage(1_000_000, 1_000_000, 0, 0)) - 18.0).abs() < 1e-9);
        // The chat-scale case: 2841 in, 512 out.
        let usd = rates.estimate_usd(usage(2841, 512, 0, 0));
        assert!((usd - (2841.0 * 3.0 + 512.0 * 15.0) / 1e6).abs() < 1e-12);
    }

    #[test]
    fn estimate_prices_cache_buckets_at_their_own_rates() {
        let rates = AgentCostRates::from_io(3.0, 15.0);
        // 1M of each bucket: 3 + 15 + 3.75 + 0.3 = $22.05.
        let usd = rates.estimate_usd(usage(1_000_000, 1_000_000, 1_000_000, 1_000_000));
        assert!((usd - 22.05).abs() < 1e-9, "{usd}");
        // A cached long-session turn: mostly cache reads at 0.1×, a small
        // fresh remainder and one cache write.
        let usd = rates.estimate_usd(usage(21, 900, 2_100, 18_000));
        let expect = (21.0 * 3.0 + 900.0 * 15.0 + 2_100.0 * 3.75 + 18_000.0 * 0.3) / 1e6;
        assert!((usd - expect).abs() < 1e-12);
    }

    #[test]
    fn formatting_switches_precision_at_a_cent() {
        assert_eq!(format_cost_usd(0.00421), "$0.0042");
        assert_eq!(format_cost_usd(0.0), "$0.0000");
        assert_eq!(format_cost_usd(0.0164), "$0.02");
        assert_eq!(format_cost_usd(1.234), "$1.23");
    }
}
