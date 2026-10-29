//! Any node's id in the rendered tree: a part, a generated copy, a segment.
//!
//! Flat (names.html "Ids"): the part that rendered it, then the key that part
//! generated for it, then the repeat copies it sits in. Grouping is never in
//! it, so regrouping moves nothing that points at a node.
//!
//! | node | id |
//! |---|---|
//! | a path (it IS its segment) | `path1` |
//! | a circle | `circle1` |
//! | one of its rings | `circle1/r2` |
//! | a repeat's part, copy 0 | `panel` |
//! | copy 2 | `panel@2` |
//! | a ring of a repeated circle, copy 2 | `circle2/r1@2` |
//! | inner copy 0 of a repeat inside outer copy 3 | `x@0@3` |
//!
//! Copies are listed **innermost first, trailing zeros dropped** (F5): copy 0
//! carries the original's ids, so `x@3` is inner copy 3 in outer copy 0 and
//! `x@0@3` is inner copy 0 in outer copy 3. Only trailing zeros go, or the two
//! would read the same.

use alloc::vec::Vec;

use crate::part::part_id::PartId;
use crate::part::segment_key::SegmentKey;

/// A node's id: `circle2/r1@2`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TreeId {
    /// The part that rendered it. Edits to its geometry land there.
    pub part: PartId,
    /// The key its part generated for it (`r2`, `s1`), or `None` when it is
    /// the part's own node.
    pub key: Option<SegmentKey>,
    /// The repeat copies it sits in, innermost first; trailing zeros are
    /// never kept (see the module doc).
    pub copies: Vec<u32>,
}

impl TreeId {
    /// A part's own node, in copy 0.
    #[must_use]
    pub fn of(part: &PartId) -> Self {
        Self {
            part: part.clone(),
            key: None,
            copies: Vec::new(),
        }
    }

    /// A segment `part` generated under itself (a ring, `r2`), in copy 0.
    #[must_use]
    pub fn generated(part: &PartId, key: SegmentKey) -> Self {
        Self {
            part: part.clone(),
            key: Some(key),
            copies: Vec::new(),
        }
    }

    /// The same node in copy `k` of a repeat round it. Innermost copies are
    /// added first, so call it from the inside out, then
    /// [`TreeId::canonical`] once every repeat round it has added its copy.
    #[must_use]
    pub fn in_copy(mut self, k: u32) -> Self {
        self.copies.push(k);
        self
    }

    /// Trailing zeros dropped: copy 0 carries the original's ids. Only once
    /// every repeat round it has added its copy (see the module doc).
    #[must_use]
    pub fn canonical(mut self) -> Self {
        self.drop_trailing_zeros();
        self
    }

    /// [`TreeId::canonical`], in place.
    pub(crate) fn drop_trailing_zeros(&mut self) {
        while self.copies.last() == Some(&0) {
            self.copies.pop();
        }
    }

    /// The same node in copy 0: what its part authored.
    #[must_use]
    pub fn original(&self) -> Self {
        Self {
            part: self.part.clone(),
            key: self.key.clone(),
            copies: Vec::new(),
        }
    }

    /// Is it a part's own node (in any copy), as opposed to a segment its
    /// part generated?
    #[must_use]
    pub fn is_part_node(&self) -> bool {
        self.key.is_none()
    }

    /// Read an id back from its text (`circle2/r1@2`). `None` when the text
    /// is not one: a bad part id or key, a copy that isn't a plain number,
    /// or a trailing `@0` (copy 0 is never written).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut pieces = text.split('@');
        let head = pieces.next()?;
        let mut copies = Vec::new();
        for piece in pieces {
            copies.push(parse_copy(piece)?);
        }
        if copies.last() == Some(&0) {
            return None;
        }
        let (part, key) = match head.split_once('/') {
            Some((part, key)) => (part, Some(SegmentKey::new(key).ok()?)),
            None => (head, None),
        };
        Some(Self {
            part: PartId::new(part).ok()?,
            key,
            copies,
        })
    }
}

impl core::fmt::Display for TreeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.part)?;
        if let Some(key) = &self.key {
            write!(f, "/{key}")?;
        }
        for k in &self.copies {
            write!(f, "@{k}")?;
        }
        Ok(())
    }
}

/// A copy number as written: plain digits, no sign, no leading zero, so the
/// text reads back to itself.
fn parse_copy(piece: &str) -> Option<u32> {
    let plain = !piece.is_empty()
        && piece.bytes().all(|b| b.is_ascii_digit())
        && (piece.len() == 1 || !piece.starts_with('0'));
    if !plain {
        return None;
    }
    piece.parse().ok()
}
