//! Words on an arrow: what it says, how loudly, and where it would like to
//! sit.

use crate::{LabelRole, Placement};

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    pub role: LabelRole,
    pub placement: Placement,
}

impl Label {
    pub fn new(text: impl Into<String>, role: LabelRole) -> Label {
        Label {
            text: text.into(),
            role,
            placement: Placement::Centre,
        }
    }

    /// What the arrow says, centred.
    pub fn primary(text: impl Into<String>) -> Label {
        Label::new(text, LabelRole::Primary)
    }

    /// A small, quiet label, centred.
    pub fn quiet(text: impl Into<String>) -> Label {
        Label::new(text, LabelRole::Quiet)
    }

    pub fn at(mut self, placement: Placement) -> Label {
        self.placement = placement;
        self
    }

    pub fn near_start(self) -> Label {
        self.at(Placement::NearStart)
    }

    pub fn near_end(self) -> Label {
        self.at(Placement::NearEnd)
    }
}
