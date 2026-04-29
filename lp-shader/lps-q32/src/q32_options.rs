//! Q32 arithmetic mode selection for code generation.
//!
//! Per-shader fast-math options wired through `lpir::CompilerConfig::q32`
//! and consumed by `lpvm-native::lower` and `lpvm-wasm::emit`. Defaults
//! to fully-saturating arithmetic; opt-in faster modes trade precision /
//! overflow protection for code-size and speed. Native and WASM produce
//! bit-identical output for the same `(mode, inputs)` so the browser
//! preview matches the device.

use core::str::FromStr;

/// Per-shader Q32 arithmetic options controlling builtin selection.
///
/// These are compiler-internal types. `lp-engine` maps `lpc_model::GlslOpts`
/// to these at the call site (Stage VI-B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Q32Options {
    pub add_sub: AddSubMode,
    pub mul: MulMode,
    pub div: DivMode,
}

impl Default for Q32Options {
    fn default() -> Self {
        Self {
            add_sub: AddSubMode::default(),
            mul: MulMode::default(),
            div: DivMode::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddSubMode {
    #[default]
    Saturating,
    Wrapping,
}

impl FromStr for AddSubMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "saturating" => Ok(AddSubMode::Saturating),
            "wrapping" => Ok(AddSubMode::Wrapping),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MulMode {
    #[default]
    Saturating,
    Wrapping,
}

impl FromStr for MulMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "saturating" => Ok(MulMode::Saturating),
            "wrapping" => Ok(MulMode::Wrapping),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DivMode {
    #[default]
    Saturating,
    Reciprocal,
}

impl FromStr for DivMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "saturating" => Ok(DivMode::Saturating),
            "reciprocal" => Ok(DivMode::Reciprocal),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_sub_from_str() {
        assert_eq!(
            "saturating".parse::<AddSubMode>(),
            Ok(AddSubMode::Saturating)
        );
        assert_eq!("wrapping".parse::<AddSubMode>(), Ok(AddSubMode::Wrapping));
        assert!("bogus".parse::<AddSubMode>().is_err());
    }

    #[test]
    fn mul_from_str() {
        assert_eq!("saturating".parse::<MulMode>(), Ok(MulMode::Saturating));
        assert_eq!("wrapping".parse::<MulMode>(), Ok(MulMode::Wrapping));
        assert!("reciprocal".parse::<MulMode>().is_err());
    }

    #[test]
    fn div_from_str() {
        assert_eq!("saturating".parse::<DivMode>(), Ok(DivMode::Saturating));
        assert_eq!("reciprocal".parse::<DivMode>(), Ok(DivMode::Reciprocal));
        assert!("wrapping".parse::<DivMode>().is_err());
    }
}
