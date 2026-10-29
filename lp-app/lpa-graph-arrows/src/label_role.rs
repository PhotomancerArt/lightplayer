//! How much a label matters: its emphasis when drawn, and who wins when
//! two want the same spot.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LabelRole {
    /// What the arrow says: a span's lamps and pixel. Placed first.
    Primary,
    /// Small and always there: a channel's name. Placed after every
    /// primary label, so it is the one that moves or drops.
    Quiet,
}
