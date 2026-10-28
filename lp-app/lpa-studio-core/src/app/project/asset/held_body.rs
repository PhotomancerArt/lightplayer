//! A node file's pending whole-file body that the board holds as slot
//! edits ([`super::def_body_slot_edits`]), so a slot edit can sit beside it.

/// What Studio keeps of a body the board holds as slot edits: the file on
/// disk the edits were made from, and the body's text, which is what the
/// file reads as until Save writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeldBody {
    /// The file on disk when the body was said as slot edits.
    pub(crate) base: Vec<u8>,
    /// What the file reads as: the body, and any edit made to it since.
    pub(crate) text: Vec<u8>,
}
