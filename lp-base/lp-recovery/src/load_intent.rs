//! A project load in progress, kept in the persistent region: what makes a
//! load transactional across a reset.
//!
//! A board does not check up front whether a project fits; it tries. Before
//! the load starts, the server writes "loading X (from P)" here
//! ([`crate::begin_project_load`]); when it finishes, one way or another,
//! it clears it ([`crate::end_project_load`]). A load that runs the board out
//! of memory resets it with the record still set, so the next boot knows
//! what happened ([`InterruptedLoad`], in the boot assessment): a switch that
//! did not fit boots the previous project again (the startup choice only
//! changes after a load succeeds), and a startup project that did not fit
//! boots with no project rather than trying again. Either way the board says
//! so in plain words, and it never blames the project's code for a load that
//! simply did not fit.
//!
//! Torn-write discipline, like the frame stack: the state word is cleared
//! first, the names written, and the state word set last, so a reset in the
//! middle never leaves a half-written record that reads as set.

use crate::crash_record::CrashCause;
use crate::frame_record::truncation_boundary;

/// Bytes kept of each project name: what the region's 1 KB budget leaves
/// (`REGION_MAX_SIZE`; the record takes the last 48 B of it). A longer name
/// is cut at a character boundary: the names are only ever words for the
/// user (the project a board boots is the server's startup choice, not this
/// record).
pub const LOAD_NAME_CAP: usize = 20;

/// A project name, cut to [`LOAD_NAME_CAP`] bytes. Plain data.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct LoadName {
    len: u8,
    bytes: [u8; LOAD_NAME_CAP],
}

impl LoadName {
    /// No name.
    pub const EMPTY: Self = Self {
        len: 0,
        bytes: [0; LOAD_NAME_CAP],
    };

    /// `name`, cut to the cap.
    pub fn new(name: &str) -> Self {
        let end = truncation_boundary(name, LOAD_NAME_CAP);
        let mut bytes = [0; LOAD_NAME_CAP];
        bytes[..end].copy_from_slice(&name.as_bytes()[..end]);
        Self {
            len: end as u8,
            bytes,
        }
    }

    /// The name; empty when there is none, or when the bytes read back are
    /// not text (a region that was never written).
    pub fn as_str(&self) -> &str {
        let len = usize::from(self.len).min(LOAD_NAME_CAP);
        core::str::from_utf8(&self.bytes[..len]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }
}

impl core::fmt::Debug for LoadName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self.as_str(), f)
    }
}

/// No load in progress.
const STATE_NONE: u32 = 0;
/// A switch: a client asked for a project while the board ran another (or
/// none).
const STATE_SWITCH: u32 = 0x4C53_5754; // "LSWT"
/// The startup load at boot.
const STATE_STARTUP: u32 = 0x4C53_5452; // "LSTR"

/// The record as it sits in the region.
#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct LoadIntent {
    target: LoadName,
    previous: LoadName,
    _pad: [u8; 2],
    state: u32,
}

impl LoadIntent {
    pub(crate) const EMPTY: Self = Self {
        target: LoadName::EMPTY,
        previous: LoadName::EMPTY,
        _pad: [0; 2],
        state: STATE_NONE,
    };

    /// A load of `target` starts; `previous` is what ran before it (empty:
    /// nothing).
    pub(crate) fn begin(&mut self, target: &str, previous: &str, at_startup: bool) {
        // SAFETY (torn writes): plain stores to our own fields; volatile so
        // the order below survives optimisation — the state word is the
        // visibility flip.
        unsafe { core::ptr::write_volatile(&raw mut self.state, STATE_NONE) };
        self.target = LoadName::new(target);
        self.previous = LoadName::new(previous);
        let state = if at_startup {
            STATE_STARTUP
        } else {
            STATE_SWITCH
        };
        unsafe { core::ptr::write_volatile(&raw mut self.state, state) };
    }

    /// The load finished (it ran, or it failed and said so).
    pub(crate) fn end(&mut self) {
        // SAFETY: as in `begin`.
        unsafe { core::ptr::write_volatile(&raw mut self.state, STATE_NONE) };
    }

    /// The load in progress, if one is: `(target, previous, at_startup)`.
    pub(crate) fn pending(&self) -> Option<(LoadName, LoadName, bool)> {
        let at_startup = match self.state {
            STATE_SWITCH => false,
            STATE_STARTUP => true,
            _ => return None,
        };
        Some((self.target, self.previous, at_startup))
    }
}

/// A project load the previous run started and never finished: the board
/// reset in the middle of it. In [`crate::BootAssessment`] and the
/// snapshot for the whole boot that follows.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct InterruptedLoad {
    /// The project that was loading.
    pub target: LoadName,
    /// What ran before it (empty: nothing).
    pub previous: LoadName,
    /// The startup load at boot, rather than a switch.
    pub at_startup: bool,
    /// What ended the run, when it left a crash record.
    pub cause: Option<CrashCause>,
}

impl InterruptedLoad {
    /// Whether this boot should skip the startup load: the startup load is
    /// the one that did not finish, and trying it again would reset again.
    pub fn skip_startup_load(&self) -> bool {
        self.at_startup
    }
}

/// Plain words for the user, e.g. `PLAYFUL Choker didn't fit in memory —
/// back on Basic`. Never a blame on the project's code: a load that ran out
/// of memory did not fit.
impl core::fmt::Display for InterruptedLoad {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let target = match self.target.as_str() {
            "" => "A project",
            name => name,
        };
        match self.cause {
            Some(CrashCause::Oom) => write!(f, "{target} didn't fit in memory")?,
            Some(_) => write!(f, "{target} stopped the board while it loaded")?,
            None => write!(f, "{target} was interrupted while it loaded")?,
        }
        if self.at_startup {
            f.write_str(" at startup, so no project is running")
        } else if self.previous.is_empty() {
            Ok(())
        } else {
            write!(f, " — back on {}", self.previous.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::ToString;

    use super::*;

    #[test]
    fn a_begun_load_is_pending_until_it_ends() {
        let mut intent = LoadIntent::EMPTY;
        assert!(intent.pending().is_none());
        intent.begin("PLAYFUL Choker", "Basic", false);
        let (target, previous, at_startup) = intent.pending().unwrap();
        assert_eq!(target.as_str(), "PLAYFUL Choker");
        assert_eq!(previous.as_str(), "Basic");
        assert!(!at_startup);
        intent.end();
        assert!(intent.pending().is_none());
    }

    #[test]
    fn a_long_name_is_cut_at_a_character_boundary() {
        let name = LoadName::new("a project with a very long name — über long");
        assert!(name.as_str().len() <= LOAD_NAME_CAP);
        assert!("a project with a very long name — über long".starts_with(name.as_str()));
    }

    #[test]
    fn the_words_say_what_happened_and_where_the_board_is() {
        let switch = InterruptedLoad {
            target: LoadName::new("Small Dome"),
            previous: LoadName::new("PLAYFUL Choker"),
            at_startup: false,
            cause: Some(CrashCause::Oom),
        };
        assert_eq!(
            switch.to_string(),
            "Small Dome didn't fit in memory — back on PLAYFUL Choker"
        );
        assert!(!switch.skip_startup_load());

        let startup = InterruptedLoad {
            previous: LoadName::EMPTY,
            at_startup: true,
            ..switch
        };
        assert_eq!(
            startup.to_string(),
            "Small Dome didn't fit in memory at startup, so no project is running"
        );
        assert!(startup.skip_startup_load());

        let hang = InterruptedLoad {
            cause: Some(CrashCause::Watchdog),
            ..switch
        };
        assert_eq!(
            hang.to_string(),
            "Small Dome stopped the board while it loaded — back on PLAYFUL Choker"
        );
    }
}
