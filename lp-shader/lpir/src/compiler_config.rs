//! Middle-end options for LPIR optimization passes (inlining, future passes).
//!
//! String keys are applied via [`CompilerConfig::apply`]; the full key namespace
//! lives here so typos surface as parse errors.

use alloc::string::String;
use core::fmt;
use core::str::FromStr;

/// Top-level LPIR pass configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompilerConfig {
    pub inline: InlineConfig,
    pub q32: lps_q32::q32_options::Q32Options,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            inline: InlineConfig::default(),
            q32: lps_q32::q32_options::Q32Options::default(),
        }
    }
}

/// Controls the inliner when it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineMode {
    /// Heuristic-based inlining (default).
    Auto,
    /// Inline everything unconditionally.
    Always,
    /// Skip all inlining.
    Never,
}

impl fmt::Display for InlineMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            InlineMode::Auto => "auto",
            InlineMode::Always => "always",
            InlineMode::Never => "never",
        })
    }
}

impl FromStr for InlineMode {
    type Err = ();

    /// Accepts lowercase names: `auto`, `always`, `never`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "auto" => Ok(InlineMode::Auto),
            "always" => Ok(InlineMode::Always),
            "never" => Ok(InlineMode::Never),
            _ => Err(()),
        }
    }
}

/// Tunables for the inline pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineConfig {
    pub mode: InlineMode,
    pub always_inline_single_site: bool,
    /// Maximum `func_weight` for "small" callees that are inlined unconditionally
    /// (subject to budgets). Empirically tuned against the rv32n cost model on the
    /// `inline-weights.glsl` corpus — see `docs/roadmaps/2026-04-15-lpir-inliner/m3.1-tune-inline-weights.md`.
    pub small_func_threshold: usize,
    pub max_growth_budget: Option<usize>,
    pub module_op_budget: Option<usize>,
}

impl Default for InlineConfig {
    fn default() -> Self {
        Self {
            mode: InlineMode::Auto,
            always_inline_single_site: true,
            small_func_threshold: 16,
            max_growth_budget: None,
            module_op_budget: None,
        }
    }
}

/// Error applying a single `compile-opt` key/value pair.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    UnknownKey { key: String },
    InvalidValue { key: String, value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::UnknownKey { key } => write!(f, "unknown config key {key:?}"),
            ConfigError::InvalidValue { key, value } => {
                write!(f, "invalid value {value:?} for config key {key:?}")
            }
        }
    }
}

impl core::error::Error for ConfigError {}

fn invalid(key: &str, value: &str) -> ConfigError {
    ConfigError::InvalidValue {
        key: String::from(key),
        value: String::from(value),
    }
}

impl CompilerConfig {
    /// Apply a single key-value override from a file directive (`compile-opt`).
    pub fn apply(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        match key.trim() {
            "inline.mode" => {
                self.inline.mode = value.trim().parse().map_err(|_| invalid(key, value))?;
            }
            "inline.always_inline_single_site" => {
                self.inline.always_inline_single_site =
                    parse_bool(value).ok_or_else(|| invalid(key, value))?;
            }
            "inline.small_func_threshold" => {
                self.inline.small_func_threshold =
                    value.trim().parse().map_err(|_| invalid(key, value))?;
            }
            "inline.max_growth_budget" => {
                self.inline.max_growth_budget =
                    Some(value.trim().parse().map_err(|_| invalid(key, value))?);
            }
            "inline.module_op_budget" => {
                self.inline.module_op_budget =
                    Some(value.trim().parse().map_err(|_| invalid(key, value))?);
            }
            "q32.add_sub" => {
                self.q32.add_sub = value.trim().parse().map_err(|_| invalid(key, value))?;
            }
            "q32.mul" => {
                self.q32.mul = value.trim().parse().map_err(|_| invalid(key, value))?;
            }
            "q32.div" => {
                self.q32.div = value.trim().parse().map_err(|_| invalid(key, value))?;
            }
            _ => {
                return Err(ConfigError::UnknownKey {
                    key: String::from(key),
                });
            }
        }
        Ok(())
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn apply_inline_mode() {
        let mut c = CompilerConfig::default();
        c.apply("inline.mode", "never").unwrap();
        assert_eq!(c.inline.mode, InlineMode::Never);
        c.apply("inline.mode", "always").unwrap();
        assert_eq!(c.inline.mode, InlineMode::Always);
    }

    #[test]
    fn apply_numeric_and_optional_budgets() {
        let mut c = CompilerConfig::default();
        c.apply("inline.small_func_threshold", "42").unwrap();
        assert_eq!(c.inline.small_func_threshold, 42);
        c.apply("inline.max_growth_budget", "100").unwrap();
        assert_eq!(c.inline.max_growth_budget, Some(100));
        c.apply("inline.module_op_budget", "7").unwrap();
        assert_eq!(c.inline.module_op_budget, Some(7));
    }

    #[test]
    fn apply_always_inline_single_site() {
        let mut c = CompilerConfig::default();
        c.apply("inline.always_inline_single_site", "false")
            .unwrap();
        assert!(!c.inline.always_inline_single_site);
        c.apply("inline.always_inline_single_site", "true").unwrap();
        assert!(c.inline.always_inline_single_site);
    }

    #[test]
    fn apply_unknown_key_errors() {
        let mut c = CompilerConfig::default();
        let r = c.apply("inline.unknown", "x");
        assert!(matches!(r, Err(ConfigError::UnknownKey { .. })));
    }

    #[test]
    fn apply_invalid_value_errors() {
        let mut c = CompilerConfig::default();
        assert!(c.apply("inline.mode", "bogus").is_err());
        assert!(c.apply("inline.small_func_threshold", "nope").is_err());
    }

    #[test]
    fn inline_mode_from_str_and_display_round_trip() {
        for s in ["auto", "always", "never"] {
            let m: InlineMode = s.parse().expect(s);
            assert_eq!(m.to_string(), s);
        }
    }

    #[test]
    fn apply_q32_add_sub() {
        let mut c = CompilerConfig::default();
        assert_eq!(c.q32.add_sub, lps_q32::q32_options::AddSubMode::Saturating);
        c.apply("q32.add_sub", "wrapping").unwrap();
        assert_eq!(c.q32.add_sub, lps_q32::q32_options::AddSubMode::Wrapping);
        c.apply("q32.add_sub", "saturating").unwrap();
        assert_eq!(c.q32.add_sub, lps_q32::q32_options::AddSubMode::Saturating);
    }

    #[test]
    fn apply_q32_mul() {
        let mut c = CompilerConfig::default();
        c.apply("q32.mul", "wrapping").unwrap();
        assert_eq!(c.q32.mul, lps_q32::q32_options::MulMode::Wrapping);
    }

    #[test]
    fn apply_q32_div() {
        let mut c = CompilerConfig::default();
        c.apply("q32.div", "reciprocal").unwrap();
        assert_eq!(c.q32.div, lps_q32::q32_options::DivMode::Reciprocal);
    }

    #[test]
    fn apply_q32_invalid_value_errors() {
        let mut c = CompilerConfig::default();
        assert!(c.apply("q32.add_sub", "bogus").is_err());
        assert!(c.apply("q32.mul", "reciprocal").is_err());
        assert!(c.apply("q32.div", "wrapping").is_err());
    }
}
