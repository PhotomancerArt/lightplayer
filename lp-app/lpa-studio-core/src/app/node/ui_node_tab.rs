//! Tabs for node pane bodies.

use crate::UiNodeSection;

/// A node pane tab.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiNodeTab {
    /// Short tab label.
    pub label: String,
    /// Tab body.
    pub body: UiNodeTabBody,
}

impl UiNodeTab {
    /// Create a tab with an explicit body.
    pub fn new(label: impl Into<String>, body: UiNodeTabBody) -> Self {
        Self {
            label: label.into(),
            body,
        }
    }

    /// Create the conventional main tab from typed sections.
    pub fn main(sections: Vec<UiNodeSection>) -> Self {
        Self::new("main", UiNodeTabBody::Sections(sections))
    }
}

/// Content rendered inside a node tab.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiNodeTabBody {
    /// Domain-aware node anatomy sections.
    Sections(Vec<UiNodeSection>),
    /// Read-only text, useful for raw JSON or diagnostics.
    Text {
        /// Heading for the text block.
        title: String,
        /// Text body.
        body: String,
    },
}

impl UiNodeTabBody {
    /// Returns true when the body has no sections or text.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Sections(sections) => sections.iter().all(UiNodeSection::is_empty),
            Self::Text { body, .. } => body.is_empty(),
        }
    }
}
