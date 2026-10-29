//! Why a format-6 document was refused, or a mapping could not be written.
//! Each refusal names the part, node or field it is about.

use alloc::string::String;

#[derive(Debug, Clone, PartialEq)]
pub enum Map2d6Error {
    /// Not JSON, or not this document's shape.
    Parse(String),
    /// Newer than this build reads, or zero.
    UnsupportedFormat { found: u32 },
    /// Format 1 to 5: today's document ([`crate::Map2dDoc`]) reads those.
    OlderFormat { found: u32 },
    /// A part id that breaks the id rule (`[a-z][a-z0-9_-]*`, at most 24
    /// bytes); `@` and `/` are named, since node ids use them.
    InvalidPartId { id: String, reason: String },
    /// Two parts claim one id.
    DuplicatePartId { id: String },
    /// `next` names a word that is no kind of part.
    UnknownCounter { word: String },
    /// `next` would mint an id a part already has: ids are never reused.
    CounterBelowId { word: String, next: u32, id: String },
    /// A circle's ring key that breaks the key rule, or one used twice.
    InvalidRingKey {
        part: String,
        key: String,
        reason: String,
    },
    /// A circle's `next_ring` would mint a key a ring already has.
    RingCounterBelowKey {
        part: String,
        next: u32,
        key: String,
    },
    /// An override keyed by text that is not a node id.
    MalformedOverrideId { id: String },
    /// A repeat of more copies than [`crate::MAX_REPEAT_COUNT`].
    TooManyCopies { part: String, count: u32 },
    /// A path's `d` that doesn't read as path data.
    PathData { part: String, reason: String },
    /// A path command this build does not draw yet: a close, a curve or an
    /// arc. Refused, never approximated.
    PathCommandNotBuilt { part: String, command: char },
    /// What a mapping holds that format 6 has no place for (lamp facts, a
    /// flow-rendered port step, a number that is not finite).
    NotWritable { reason: String },
}

impl core::fmt::Display for Map2d6Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(reason) => write!(f, "invalid map2d format 6 document: {reason}"),
            Self::UnsupportedFormat { found } => write!(
                f,
                "unsupported map2d format {found} (this build reads up to 6)"
            ),
            Self::OlderFormat { found } => write!(
                f,
                "map2d format {found} is the older layout document; this reader reads format 6"
            ),
            Self::InvalidPartId { id, reason } => write!(f, "part id {id:?}: {reason}"),
            Self::DuplicatePartId { id } => write!(f, "two parts are called {id:?}"),
            Self::UnknownCounter { word } => {
                write!(f, "next: {word:?} is not a kind of part")
            }
            Self::CounterBelowId { word, next, id } => write!(
                f,
                "next: {word} is {next}, but part {id:?} exists (ids are never reused)"
            ),
            Self::InvalidRingKey { part, key, reason } => {
                write!(f, "part {part:?}: ring key {key:?}: {reason}")
            }
            Self::RingCounterBelowKey { part, next, key } => write!(
                f,
                "part {part:?}: next_ring is {next}, but ring {key:?} exists (keys are never reused)"
            ),
            Self::MalformedOverrideId { id } => {
                write!(f, "overrides: {id:?} is not a node id")
            }
            Self::TooManyCopies { part, count } => write!(
                f,
                "part {part:?}: a repeat of {count} copies is more than {}",
                crate::MAX_REPEAT_COUNT
            ),
            Self::PathData { part, reason } => write!(f, "part {part:?}: path data: {reason}"),
            Self::PathCommandNotBuilt { part, command } => write!(
                f,
                "part {part:?}: path command {command:?} ({}) is not built: this build draws straight lines only",
                command_name(*command)
            ),
            Self::NotWritable { reason } => {
                write!(f, "can't write map2d format 6: {reason}")
            }
        }
    }
}

impl core::error::Error for Map2d6Error {}

/// What an SVG path command letter draws, in words.
fn command_name(command: char) -> &'static str {
    match command.to_ascii_uppercase() {
        'Z' => "close",
        'C' | 'S' => "cubic curve",
        'Q' | 'T' => "quadratic curve",
        'A' => "arc",
        _ => "unknown",
    }
}
