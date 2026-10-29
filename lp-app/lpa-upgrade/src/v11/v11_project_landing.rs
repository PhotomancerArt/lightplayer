//! The frozen landing's answer: for each fixture of a format-11 project,
//! its segments and where each one's lamps land today.

use super::v11_segments::SegmentSource;

/// Where every fixture lamp of a format-11 project lands today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLanding {
    /// Every fixture, in load order.
    pub fixtures: Vec<FixtureLanding>,
    /// Every output, in load order.
    pub outputs: Vec<OutputLanding>,
}

/// One fixture's segments and where each lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureLanding {
    /// The fixture's definition file, from the project root
    /// (`/dome/dome.json`): its identity, stable across the upgrade.
    pub node: String,
    /// The bus channel it writes.
    pub channel: String,
    /// Its lamp count (the control product's extent).
    pub lamps: u32,
    /// Its map2d document, when it has one.
    pub mapping_file: Option<String>,
    /// Its patch document, when it has one.
    pub patch_file: Option<String>,
    pub layout: FixtureLayout,
}

/// A fixture's layout as the frozen landing read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixtureLayout {
    /// Segments in lamp order, each with its landing.
    Segments(Vec<SegmentLanding>),
    /// Its map2d is already at format 6 or newer: nothing here reads it.
    AlreadyFormat6,
}

/// One segment and where its lamps land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentLanding {
    pub source: SegmentSource,
    /// The segment's first lamp, in the fixture's lamp order.
    pub first_lamp: u32,
    pub lamps: u32,
    pub landing: Landing,
}

/// Where one segment's lamps land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landing {
    /// Every lamp on one port of one output, filling a window of `lamps`
    /// lamps from `first_channel`: segment lamp `j` sits at channel
    /// `first_channel + 3 · ((j' + rotation) mod lamps)`, `j'` being `j`
    /// or, reversed, `lamps − 1 − j`.
    Landed {
        /// The output's definition file.
        output: String,
        port: u32,
        /// The window's first channel within the port.
        first_channel: u32,
        reversed: bool,
        /// In lamps, `< lamps`.
        rotation: u32,
    },
    /// No lamp of the segment reaches a wire.
    Dark { why: DarkReason },
    /// The segment's lamps are not one window on one port: a range or a
    /// sub-span cuts it, a port boundary falls inside it, or only part of
    /// it is dark. `pieces` say where each lamp went.
    Cut {
        why: String,
        pieces: Vec<LandedPiece>,
    },
}

/// A run of a cut segment's lamps landing one after another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandedPiece {
    /// The piece's first lamp, in the fixture's lamp order.
    pub first_lamp: u32,
    pub lamps: u32,
    pub output: String,
    pub port: u32,
    /// The channel the piece's first lamp lands on.
    pub first_channel: u32,
    /// `3` forward, `-3` reversed.
    pub step: i32,
}

/// Why a lamp lands nowhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DarkReason {
    /// No output reads the fixture's bus channel.
    NoOutputReads { channel: String },
    /// Under `flow: manual` (or a patch the kernel refuses beside it), no
    /// row places the lamp.
    NotPatched,
    /// Its row names an output that is not reading this fixture (or that no
    /// output is named).
    RowNamesAnotherOutput { name: String },
    /// It falls past the output's last port, or the output drives no wire
    /// (a port before the last omits its count).
    OnNoPort { output: String },
}

/// One output, as patch rows and starts address it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLanding {
    /// The output's definition file.
    pub node: String,
    pub name: Option<String>,
    pub channel: String,
}

impl std::fmt::Display for DarkReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutputReads { channel } => write!(f, "no output reads bus:{channel}"),
            Self::NotPatched => f.write_str("its patch places it nowhere"),
            Self::RowNamesAnotherOutput { name } => {
                write!(f, "its row names output {name:?}, which does not read it")
            }
            Self::OnNoPort { output } => write!(f, "it falls on no port of {output}"),
        }
    }
}
