//! The relay's last picture of each board. Sans-IO; part of the hub.
//!
//! A protocol 2 board sends its picture (`lpc_relay::RelayPicture`) at the
//! cadence the hub asks; the hub keeps the latest one per board here, beside
//! presence, so any number of readers share one upstream and nobody takes
//! the board's one network slot to see it.
//!
//! **Memory only** (the plan's D4, following the vision: "the cloud keeps
//! the latest frame in memory beside presence"). An entry outlives its
//! board — marked offline — until the process ends; a deploy drops every
//! entry, and online boards refill theirs within seconds (each sends a
//! picture as soon as the new hub asks). Persisting a board's last picture
//! is new persisted cloud data, and belongs with the account's board list
//! (the boards-and-projects roadmap's M7), designed once, there.
//!
//! The rules, each pinned by a test (here and in `relay_hub.rs`):
//!
//! - **Readers** are the accounts the board proved when its picture
//!   arrived; a board registering again with other accounts changes them
//!   at once, so a removed account stops reading the old picture.
//! - **At most [`MAX_CACHED_PICTURES`] boards**: when full, the oldest
//!   offline entry goes first, then the oldest.
//! - **A picture less than [`MIN_PICTURE_GAP_S`] after the board's last
//!   accepted one is dropped**: a guard against a broken board, never a
//!   close.
//! - **`seq`** moves on every picture accepted, across every board, from a
//!   base the process picks at start, so a reader holding a `seq` from an
//!   earlier process never mistakes a new picture for the one it has.

use std::collections::HashMap;

use lpc_history::PrefixedUid;
use lpc_relay::{RelayBoardId, RelayPicture, RelayProject};

/// The most boards the cache holds.
pub const MAX_CACHED_PICTURES: usize = 4096;

/// The shortest gap between two pictures of one board the cache accepts,
/// in seconds. A board asks for at most four a second (its own clamp).
pub const MIN_PICTURE_GAP_S: f64 = 0.2;

/// One board's last picture.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedPicture {
    /// The accounts that may read it: the board's proven accounts when the
    /// picture arrived (`BoardAccounts.users`), or since it last
    /// registered.
    pub accounts: Vec<PrefixedUid>,
    pub picture: RelayPicture,
    /// The board's project as it last reported it. Kept, not yet read: the
    /// tags are for matching against the account's projects later.
    pub project: Option<RelayProject>,
    /// Moves on every picture accepted; a reader's "do I have the latest?".
    pub seq: u64,
    /// When it arrived, f64 epoch seconds.
    pub at: f64,
    pub online: bool,
}

/// See the module doc.
#[derive(Debug)]
pub struct PictureCache {
    entries: HashMap<RelayBoardId, CachedPicture>,
    next_seq: u64,
}

impl PictureCache {
    /// An empty cache whose first picture gets `first_seq`.
    #[must_use]
    pub fn new(first_seq: u64) -> Self {
        Self {
            entries: HashMap::new(),
            next_seq: first_seq,
        }
    }

    /// A picture from online board `id`, at `now`. `false` when it was
    /// dropped as too soon after the last.
    pub fn put(
        &mut self,
        id: RelayBoardId,
        accounts: &[PrefixedUid],
        picture: RelayPicture,
        project: Option<RelayProject>,
        now: f64,
    ) -> bool {
        if let Some(entry) = self.entries.get(&id)
            && now - entry.at < MIN_PICTURE_GAP_S
        {
            return false;
        }
        if !self.entries.contains_key(&id) && self.entries.len() >= MAX_CACHED_PICTURES {
            self.evict_one();
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.insert(
            id,
            CachedPicture {
                accounts: accounts.to_vec(),
                picture,
                project,
                seq,
                at: now,
                online: true,
            },
        );
        true
    }

    /// Board `id` reported its project.
    pub fn set_project(&mut self, id: RelayBoardId, project: Option<RelayProject>) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.project = project;
        }
    }

    /// Board `id` registered with `accounts`: they, and only they, read its
    /// picture from now on, and it is online.
    pub fn registered(&mut self, id: RelayBoardId, accounts: &[PrefixedUid]) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.accounts = accounts.to_vec();
            entry.online = true;
        }
    }

    /// Board `id` left: its picture stays, marked offline.
    pub fn offline(&mut self, id: RelayBoardId) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.online = false;
        }
    }

    /// Board `id`'s picture, if `user` may read it.
    #[must_use]
    pub fn readable(&self, id: RelayBoardId, user: PrefixedUid) -> Option<&CachedPicture> {
        self.entries
            .get(&id)
            .filter(|entry| entry.accounts.contains(&user))
    }

    /// Board `id`'s entry, whoever asks (the hub's own bookkeeping).
    #[must_use]
    pub fn get(&self, id: RelayBoardId) -> Option<&CachedPicture> {
        self.entries.get(&id)
    }

    /// How many boards have a picture.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no board has a picture.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forget every picture (the process is going away).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Make room: the oldest offline entry, else the oldest.
    fn evict_one(&mut self) {
        let oldest = |online: bool| {
            self.entries
                .iter()
                .filter(|(_, entry)| entry.online == online)
                .min_by(|(_, a), (_, b)| a.at.total_cmp(&b.at))
                .map(|(id, _)| *id)
        };
        if let Some(id) = oldest(false).or_else(|| oldest(true)) {
            self.entries.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;

    #[test]
    fn a_picture_too_soon_after_the_last_is_dropped() {
        let mut cache = PictureCache::new(1);
        assert!(cache.put(id(1), &[user(1)], picture(1), None, 10.0));
        assert!(!cache.put(id(1), &[user(1)], picture(2), None, 10.1));
        assert_eq!(cache.get(id(1)).unwrap().picture, picture(1));
        assert!(cache.put(id(1), &[user(1)], picture(3), None, 10.25));
        assert_eq!(cache.get(id(1)).unwrap().seq, 2, "the drop took no seq");
        assert!(
            cache.put(id(2), &[user(1)], picture(4), None, 10.26),
            "another board is not held back"
        );
    }

    #[test]
    fn seq_moves_on_every_picture_from_its_base() {
        let mut cache = PictureCache::new(1_000);
        cache.put(id(1), &[user(1)], picture(1), None, 0.0);
        cache.put(id(2), &[user(1)], picture(1), None, 0.0);
        cache.put(id(1), &[user(1)], picture(2), None, 1.0);
        assert_eq!(cache.get(id(1)).unwrap().seq, 1_002);
        assert_eq!(cache.get(id(2)).unwrap().seq, 1_001);
    }

    #[test]
    fn eviction_takes_the_oldest_offline_entry_first_then_the_oldest() {
        let mut cache = PictureCache::new(1);
        for n in 0..MAX_CACHED_PICTURES as u32 {
            cache.put(id(n), &[user(1)], picture(1), None, f64::from(n));
        }
        cache.offline(id(10));
        cache.offline(id(20));
        cache.put(id(9_000), &[user(1)], picture(1), None, 9_000.0);
        assert_eq!(cache.len(), MAX_CACHED_PICTURES);
        assert!(cache.get(id(10)).is_none(), "the oldest offline went");
        assert!(cache.get(id(0)).is_some(), "older, but online");
        cache.put(id(9_001), &[user(1)], picture(1), None, 9_001.0);
        assert!(cache.get(id(20)).is_none());
        cache.put(id(9_002), &[user(1)], picture(1), None, 9_002.0);
        assert!(cache.get(id(0)).is_none(), "none offline: the oldest");
        assert_eq!(cache.len(), MAX_CACHED_PICTURES);
    }

    fn id(n: u32) -> RelayBoardId {
        let [a, b, c, d] = n.to_be_bytes();
        RelayBoardId([0x02, 0, a, b, c, d])
    }

    fn user(n: u8) -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::User, &[n; 16])
    }

    fn picture(shade: u8) -> RelayPicture {
        RelayPicture {
            outputs: vec![1],
            colors: vec![shade, shade, shade],
        }
    }
}
