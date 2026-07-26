//! Estimated-cost math for the chat usage footnote. Pure: a small dated
//! pricing table plus formatting; settings rate overrides win over the
//! table, and an unknown model with no override yields no estimate.

/// Known model prices by id prefix — `(prefix, $/MTok input, $/MTok
/// output)`. Prices as of 2026-07. Prefix matching keeps dated ids
/// (`claude-sonnet-5-20260203`-style) matched to their family.
const MODEL_PRICES: &[(&str, f64, f64)] = &[
    ("claude-sonnet-5", 3.0, 15.0),
    ("claude-opus-4-8", 5.0, 25.0),
    ("claude-opus-4-7", 5.0, 25.0),
    ("claude-haiku-4-5", 1.0, 5.0),
];

/// Cost rates for one model, $ per million tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgentCostRates {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl AgentCostRates {
    /// Table rates for a model id (longest configured prefix wins), or
    /// `None` for unknown models.
    pub fn for_model(model: &str) -> Option<Self> {
        MODEL_PRICES
            .iter()
            .filter(|(prefix, _, _)| model.starts_with(prefix))
            .max_by_key(|(prefix, _, _)| prefix.len())
            .map(|&(_, input, output)| Self {
                input_per_mtok: input,
                output_per_mtok: output,
            })
    }

    /// The estimated cost in dollars for a token count.
    pub fn estimate_usd(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        (f64::from(input_tokens) * self.input_per_mtok
            + f64::from(output_tokens) * self.output_per_mtok)
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
    fn unknown_model_has_no_rates() {
        assert_eq!(AgentCostRates::for_model("gpt-5.2-mini"), None);
        assert_eq!(AgentCostRates::for_model(""), None);
    }

    #[test]
    fn estimate_math_is_per_million_tokens() {
        let rates = AgentCostRates {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        };
        // 1M in + 1M out = $18.
        assert!((rates.estimate_usd(1_000_000, 1_000_000) - 18.0).abs() < 1e-9);
        // The chat-scale case: 2841 in, 512 out.
        let usd = rates.estimate_usd(2841, 512);
        assert!((usd - (2841.0 * 3.0 + 512.0 * 15.0) / 1e6).abs() < 1e-12);
    }

    #[test]
    fn formatting_switches_precision_at_a_cent() {
        assert_eq!(format_cost_usd(0.00421), "$0.0042");
        assert_eq!(format_cost_usd(0.0), "$0.0000");
        assert_eq!(format_cost_usd(0.0164), "$0.02");
        assert_eq!(format_cost_usd(1.234), "$1.23");
    }
}
