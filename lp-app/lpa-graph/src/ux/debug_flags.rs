//! The Graph's debug switches: things being tried, reachable from the
//! Debug control at the toolbar's right end, never in the way. View state:
//! the page remembers them per browser; never an offer, never in the
//! project.

use crate::ux::ui::SizeClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DebugFlags {
    /// Dragging a node pins it, and the layout honours pins (D48). Off by
    /// default: the layout is automatic only, on trial (D64), and every
    /// reach for a drag is a finding.
    pub pinning: bool,
    /// Draw as if the screen were this size, the way devtools' device mode
    /// does (architecture §7). `None`: the screen's own.
    pub size_class: Option<SizeClass>,
    /// How the inspector shows a node's start against its flow (lean Q82).
    /// G3 picks one; the other is deleted (Q98).
    pub presentation: Presentation,
    /// The inspector ends with the patcher's own steps, field by field
    /// (Keep, Set, Add): for learning the model, under either presentation.
    pub cursor_ops: bool,
}

/// How the inspector shows where a node's start comes from (lean Q82).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Presentation {
    /// Each value, with where it came from ("from circle1's flow: one per
    /// port"; "set here", with Back to the flow).
    #[default]
    A,
    /// The start the flow gives the node, as one quiet line, then only the
    /// fields set here, each with Back to the flow
    /// (`presentation_b.rs`).
    B,
}

impl Presentation {
    pub const ALL: [Presentation; 2] = [Presentation::A, Presentation::B];

    /// The word on the switch, and what the page remembers.
    pub fn word(self) -> &'static str {
        match self {
            Presentation::A => "a",
            Presentation::B => "b",
        }
    }

    /// The switch's word back; anything else is the default.
    pub fn from_word(word: &str) -> Presentation {
        match word {
            "b" => Presentation::B,
            _ => Presentation::A,
        }
    }

    /// What it shows, for the switch's tooltip.
    pub fn words(self) -> &'static str {
        match self {
            Presentation::A => "Each value says where it came from",
            Presentation::B => "The flow's start, then only what is set here",
        }
    }
}
