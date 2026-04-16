//! Q32 arithmetic mode selection for code generation.

/// Per-shader Q32 arithmetic options controlling builtin selection.
///
/// These are compiler-internal types. `lp-engine` maps `lp_model::GlslOpts`
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MulMode {
    #[default]
    Saturating,
    Wrapping,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DivMode {
    #[default]
    Saturating,
    Reciprocal,
}
