//! SPIKE (plan `lp2025/2026-09-25-0006-learned-wire-dictionary`): a
//! per-connection **learned table**, HPACK-style.
//!
//! The format's key and value codes index `seed ++ learned`: the first
//! `seed.len()` codes are the injected [`Dictionary`](crate::Dictionary), the
//! rest this table. Every inline key and every qualifying inline string
//! *defines* the next learned entry as a side effect of being sent (HPACK's
//! "literal with incremental indexing"), so a name costs its inline bytes once
//! per connection and a code after that. No new tag is needed.
//!
//! Both ends must learn identically, so what qualifies and the capacities are
//! part of the protocol: [`LearnedTable`]'s const parameters and the `LEARN_*`
//! constants here. When a side is full, learning stops (no eviction).
//!
//! **Commit/rollback.** A frame's definitions are tentative. The encoder side
//! takes a [`LearnMark`] before a frame and [`LearnStore::truncate`]s back to
//! it when the frame is not sent (a JSON fallback, or a write abandoned); the
//! decoder side truncates when a frame does not decode. Truncation rebuilds
//! the hash indexes (rare path).
//!
//! **Frame header.** A learned frame starts with the table's epoch (one byte)
//! and a 16-bit fold of a rolling hash over everything the table learned (and
//! every first sighting it remembered), in order. A reader whose own table
//! disagrees must not decode the frame: see [`read_header`]. A count alone is
//! not enough: after a torn frame the two sides can learn *different* entries
//! and still hold the same number.

use crate::pack_dictionary::fnv1a;

/// An empty hash slot. Slots hold entry index + 1, so a new table is all
/// zeroes and a static one lands in `.bss`, not flash-backed `.data`.
const SLOT_EMPTY: u16 = 0;

/// Longest key the table learns.
pub const LEARN_KEY_MAX_LEN: usize = 48;
/// Shortest value string the table learns.
pub const LEARN_VALUE_MIN_LEN: usize = 2;
/// Longest value string the table learns (the inline-length tag's reach).
pub const LEARN_VALUE_MAX_LEN: usize = 31;

/// A table position: entries on each side and text bytes used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LearnMark {
    /// Keys held.
    pub keys: usize,
    /// Values held.
    pub values: usize,
    /// Text bytes held.
    pub text: usize,
    /// First sightings remembered (second-sighting policy).
    pub seen: usize,
    /// The table's rolling state hash.
    pub state: u32,
}

/// The state hash of an empty table (zero, so a new table is all-zero).
pub const STATE_EMPTY: u32 = 0;

fn mix(h: u32, kind: u8, bytes: &[u8]) -> u32 {
    let mut h = (h ^ u32::from(kind)).wrapping_mul(0x0100_0193);
    for &b in bytes {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    (h ^ bytes.len() as u32).wrapping_mul(0x0100_0193)
}

/// What the encoder and decoder need of a learned table. Object-safe, so a
/// table's capacities stay out of the codec's types.
pub trait LearnStore {
    /// The epoch frames are coded in.
    fn epoch(&self) -> u8;
    /// Where the table stands now.
    fn mark(&self) -> LearnMark;
    /// Drop every entry after `mark`.
    fn truncate(&mut self, mark: LearnMark);
    /// Empty the table and start a new epoch.
    fn reset(&mut self, epoch: u8);
    /// The index of learned key `text`.
    fn find_key(&self, text: &[u8]) -> Option<usize>;
    /// The index of learned value `text`.
    fn find_value(&self, text: &[u8]) -> Option<usize>;
    /// Learned key `i`.
    fn key(&self, i: usize) -> Option<&[u8]>;
    /// Learned value `i`.
    fn value(&self, i: usize) -> Option<&[u8]>;
    /// An inline key went by: learn it if it qualifies and there is room.
    fn learn_key(&mut self, text: &[u8]);
    /// An inline string went by: learn it if it qualifies and there is room.
    fn learn_value(&mut self, text: &[u8]);
}

/// One side (keys or values) of a [`LearnedTable`].
#[derive(Clone)]
struct Side<const N: usize, const H: usize> {
    start: [u16; N],
    len: [u8; N],
    count: usize,
    hash: [u16; H],
}

impl<const N: usize, const H: usize> Side<N, H> {
    const NEW: Self = Self {
        start: [0; N],
        len: [0; N],
        count: 0,
        hash: [SLOT_EMPTY; H],
    };

    fn get<'t>(&self, text: &'t [u8], i: usize) -> Option<&'t [u8]> {
        if i >= self.count {
            return None;
        }
        let a = usize::from(self.start[i]);
        text.get(a..a + usize::from(self.len[i]))
    }

    fn find(&self, text: &[u8], needle: &[u8]) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        let mask = H - 1;
        let mut slot = fnv1a(needle) as usize & mask;
        for _ in 0..H {
            let v = self.hash[slot];
            if v == SLOT_EMPTY {
                return None;
            }
            let i = usize::from(v - 1);
            if self.get(text, i) == Some(needle) {
                return Some(i);
            }
            slot = (slot + 1) & mask;
        }
        None
    }

    fn index(&mut self, text: &[u8], i: usize) {
        let mask = H - 1;
        let e = self.get(text, i).unwrap_or(&[]);
        let mut slot = fnv1a(e) as usize & mask;
        while self.hash[slot] != SLOT_EMPTY {
            slot = (slot + 1) & mask;
        }
        self.hash[slot] = i as u16 + 1;
    }

    fn rebuild(&mut self, text: &[u8], count: usize) {
        self.count = count.min(self.count);
        self.hash = [SLOT_EMPTY; H];
        for i in 0..self.count {
            self.index(text, i);
        }
    }
}

/// Value learning: every qualifying inline string.
pub const VALUES_ALL: u8 = 0;
/// Value learning: none (keys only).
pub const VALUES_NONE: u8 = 1;
/// Value learning: a string is learned the second time it goes inline. A log
/// of `SEEN` 32-bit hashes remembers first sightings; it is part of the table
/// state (rolled back with it), and when full no new first sighting is
/// remembered.
pub const VALUES_SECOND_SIGHTING: u8 = 2;

/// A learned table with fixed capacities: `TEXT` bytes of text shared by
/// both sides, `K` keys (hash `KH`, a power of two above `K`), `V` values
/// (hash `VH`), value policy `VPOL`, and a first-sighting set of `SEEN`
/// hashes (a power of two, or 0).
#[derive(Clone)]
pub struct LearnedTable<
    const TEXT: usize,
    const K: usize,
    const KH: usize,
    const V: usize,
    const VH: usize,
    const VPOL: u8 = VALUES_ALL,
    const SEEN: usize = 0,
> {
    text: [u8; TEXT],
    text_len: usize,
    keys: Side<K, KH>,
    values: Side<V, VH>,
    epoch: u8,
    seen: [u16; SEEN],
    seen_count: usize,
    /// A rolling hash of everything learned and every first sighting
    /// remembered, in order: what the frame header carries.
    state: u32,
}

impl<
    const TEXT: usize,
    const K: usize,
    const KH: usize,
    const V: usize,
    const VH: usize,
    const VPOL: u8,
    const SEEN: usize,
> LearnedTable<TEXT, K, KH, V, VH, VPOL, SEEN>
{
    /// An empty table in epoch 0.
    pub const NEW: Self = Self {
        text: [0; TEXT],
        text_len: 0,
        keys: Side::NEW,
        values: Side::NEW,
        epoch: 0,
        seen: [0; SEEN],
        seen_count: 0,
        state: STATE_EMPTY,
    };

    /// Whether `s` was seen inline before; remembers it if not (and there is
    /// room). A linear scan: it runs only for strings not yet learned.
    fn second_sighting(&mut self, s: &[u8]) -> bool {
        let f = fnv1a(s);
        let h = (f ^ (f >> 16)) as u16;
        if self.seen[..self.seen_count].contains(&h) {
            return true;
        }
        if self.seen_count < SEEN {
            self.seen[self.seen_count] = h;
            self.seen_count += 1;
            self.state = mix(self.state, b's', &h.to_le_bytes());
        }
        false
    }

    fn push_text(&mut self, s: &[u8]) -> Option<u16> {
        let at = self.text_len;
        let end = at + s.len();
        if end > TEXT || end > usize::from(u16::MAX) {
            return None;
        }
        self.text[at..end].copy_from_slice(s);
        self.text_len = end;
        Some(at as u16)
    }
}

impl<
    const TEXT: usize,
    const K: usize,
    const KH: usize,
    const V: usize,
    const VH: usize,
    const VPOL: u8,
    const SEEN: usize,
> LearnStore for LearnedTable<TEXT, K, KH, V, VH, VPOL, SEEN>
{
    fn epoch(&self) -> u8 {
        self.epoch
    }

    fn mark(&self) -> LearnMark {
        LearnMark {
            keys: self.keys.count,
            values: self.values.count,
            text: self.text_len,
            seen: self.seen_count,
            state: self.state,
        }
    }

    fn truncate(&mut self, mark: LearnMark) {
        if mark == self.mark() {
            return;
        }
        self.seen_count = mark.seen.min(self.seen_count);
        self.state = mark.state;
        self.text_len = mark.text.min(self.text_len);
        let text = &self.text[..self.text_len];
        self.keys.rebuild(text, mark.keys);
        self.values.rebuild(text, mark.values);
    }

    fn reset(&mut self, epoch: u8) {
        self.truncate(LearnMark {
            state: STATE_EMPTY,
            ..LearnMark::default()
        });
        self.epoch = epoch;
    }

    fn find_key(&self, text: &[u8]) -> Option<usize> {
        self.keys.find(&self.text, text)
    }

    fn find_value(&self, text: &[u8]) -> Option<usize> {
        self.values.find(&self.text, text)
    }

    fn key(&self, i: usize) -> Option<&[u8]> {
        self.keys.get(&self.text, i)
    }

    fn value(&self, i: usize) -> Option<&[u8]> {
        self.values.get(&self.text, i)
    }

    fn learn_key(&mut self, s: &[u8]) {
        if s.is_empty() || s.len() > LEARN_KEY_MAX_LEN || self.keys.count >= K {
            return;
        }
        if let Some(at) = self.push_text(s) {
            let i = self.keys.count;
            self.keys.start[i] = at;
            self.keys.len[i] = s.len() as u8;
            self.keys.count += 1;
            self.keys.index(&self.text, i);
            self.state = mix(self.state, b'k', s);
        }
    }

    fn learn_value(&mut self, s: &[u8]) {
        if VPOL == VALUES_NONE
            || !(LEARN_VALUE_MIN_LEN..=LEARN_VALUE_MAX_LEN).contains(&s.len())
            || self.values.count >= V
        {
            return;
        }
        if VPOL == VALUES_SECOND_SIGHTING && !self.second_sighting(s) {
            return;
        }
        if let Some(at) = self.push_text(s) {
            let i = self.values.count;
            self.values.start[i] = at;
            self.values.len[i] = s.len() as u8;
            self.values.count += 1;
            self.values.index(&self.text, i);
            self.state = mix(self.state, b'v', s);
        }
    }
}

/// Write a learned frame's header (epoch, then the table's state hash folded
/// to 16 bits, little-endian) into `out`. Returns its length (3), or `None`
/// when `out` is too small.
pub fn write_header(out: &mut [u8], store: &dyn LearnStore) -> Option<usize> {
    let s = fold(store.mark().state);
    out.get_mut(..3)?
        .copy_from_slice(&[store.epoch(), s as u8, (s >> 8) as u8]);
    Some(3)
}

fn fold(state: u32) -> u16 {
    (state ^ (state >> 16)) as u16
}

/// Why a learned frame's header does not match the reader's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderMismatch {
    /// The frame is shorter than a header.
    Truncated,
    /// The board is in another epoch: the reader missed a reset.
    Epoch {
        /// The frame's epoch.
        frame: u8,
        /// The reader's.
        reader: u8,
    },
    /// Same epoch, different table: a frame that changed the table was lost
    /// or torn without the board knowing, and the two sides have diverged.
    State {
        /// The frame's folded state.
        frame: u16,
        /// The reader's.
        reader: u16,
    },
}

/// Read a learned frame's header and check it against `store`. On a match
/// returns the header length (the value starts there).
///
/// A frame of a **new** epoch whose state is the empty table's is the board's
/// announced reset: the reader resets `store` to that epoch and decodes it.
pub fn read_header(frame: &[u8], store: &mut dyn LearnStore) -> Result<usize, HeaderMismatch> {
    let h = frame.get(..3).ok_or(HeaderMismatch::Truncated)?;
    let (epoch, state) = (h[0], u16::from_le_bytes([h[1], h[2]]));
    if epoch != store.epoch() {
        if state == fold(STATE_EMPTY) {
            store.reset(epoch);
            return Ok(3);
        }
        return Err(HeaderMismatch::Epoch {
            frame: epoch,
            reader: store.epoch(),
        });
    }
    let have = fold(store.mark().state);
    if state != have {
        return Err(HeaderMismatch::State {
            frame: state,
            reader: have,
        });
    }
    Ok(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Small = LearnedTable<64, 4, 8, 4, 8>;

    #[test]
    fn learns_until_full_then_stops() {
        let mut t = Small::NEW;
        for k in [&b"alpha"[..], b"beta", b"gamma", b"delta", b"eps"] {
            t.learn_key(k);
        }
        assert_eq!(t.mark().keys, 4);
        assert_eq!(t.find_key(b"gamma"), Some(2));
        assert_eq!(t.find_key(b"eps"), None);
        t.learn_value(b"x"); // too short
        t.learn_value(b"xy");
        assert_eq!(t.find_value(b"xy"), Some(0));
    }

    #[test]
    fn truncate_forgets_and_reindexes() {
        let mut t = Small::NEW;
        t.learn_key(b"a1");
        let m = t.mark();
        t.learn_key(b"b2");
        t.learn_value(b"v3");
        t.truncate(m);
        assert_eq!(t.find_key(b"b2"), None);
        assert_eq!(t.find_value(b"v3"), None);
        assert_eq!(t.find_key(b"a1"), Some(0));
        t.learn_key(b"c3");
        assert_eq!(t.find_key(b"c3"), Some(1));
    }

    #[test]
    fn header_detects_divergence_and_accepts_announced_reset() {
        let mut board = Small::NEW;
        let mut host = Small::NEW;
        board.learn_key(b"k1");
        let mut h = [0u8; 8];
        let n = write_header(&mut h, &board).unwrap();
        assert!(matches!(
            read_header(&h[..n], &mut host),
            Err(HeaderMismatch::State { .. })
        ));
        board.reset(1);
        let n = write_header(&mut h, &board).unwrap();
        assert_eq!(read_header(&h[..n], &mut host), Ok(n));
        assert_eq!(host.epoch(), 1);
    }
}
