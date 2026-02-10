//! Q32 transform options: per-op control for speed vs accuracy.

use lp_model::glsl_opts::{AddSubMode, DivMode, MulMode};

/// Granular control over Q32 arithmetic transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Q32Options {
    pub add_sub: AddSubMode,
    pub mul: MulMode,
    pub div: DivMode,
}

impl Default for Q32Options {
    fn default() -> Self {
        Self {
            add_sub: AddSubMode::Saturating,
            mul: MulMode::Saturating,
            div: DivMode::Saturating,
        }
    }
}

impl Q32Options {
    pub fn builder() -> Q32OptionsBuilder {
        Q32OptionsBuilder::new()
    }
}

/// Builder for Q32Options.
#[derive(Debug, Default)]
pub struct Q32OptionsBuilder {
    add_sub: AddSubMode,
    mul: MulMode,
    div: DivMode,
}

impl Q32OptionsBuilder {
    fn new() -> Self {
        Self::default()
    }

    pub fn add_sub(mut self, m: AddSubMode) -> Self {
        self.add_sub = m;
        self
    }

    pub fn mul(mut self, m: MulMode) -> Self {
        self.mul = m;
        self
    }

    pub fn div(mut self, m: DivMode) -> Self {
        self.div = m;
        self
    }

    pub fn build(self) -> Q32Options {
        Q32Options {
            add_sub: self.add_sub,
            mul: self.mul,
            div: self.div,
        }
    }
}
