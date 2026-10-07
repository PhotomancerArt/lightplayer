//! A run's pace: whether a board's guest clock may run ahead of wall time.
//!
//! An idle board's guest clock runs far ahead of the host's (a `wfi` skips
//! straight to the next timer), which is what a unit test wants and what a
//! person watching a pattern, or a host talking to the board over its LAN,
//! does not. So the pace is an explicit choice of the host that runs the
//! board (`lp-cli emu run --pace`, `emu serve`'s `pace=`):
//!
//! - [`Pace::Realtime`]: 1×. The board never runs ahead of wall time, for
//!   the whole run, host or no host. A board slower than wall time (a shader
//!   compiling) is not hurried.
//! - [`Pace::Max`]: as fast as the host can go. Never paced, even with a host
//!   connected through a port forward.
//! - **Unset** (`None`, every existing run): realtime while a host is
//!   connected through a port forward, otherwise as fast as it goes
//!   (`docs/defects/2026-10-06-an-emulated-boards-clock-outran-its-lan-host.md`).
//!
//! The mechanism is the LAN's ([`super::lan_host_pace`], [`super::SharedLan`]):
//! a board is held to wall time at its LAN pump, so a pace needs the board on
//! a self-driven or wall-clock LAN (the network seam engaged). A run with no
//! LAN refuses [`Pace::Realtime`] rather than running unpaced under a label
//! that says otherwise; [`Pace::Max`] needs nothing.
//!
//! **The label.** A run whose pace was set says so after its seam atoms,
//! `lp-emu:esp32c6:t1+net=lan@pace=realtime` ([`Pace::label_suffix`]). Not a
//! `+` atom: a pace is not a seam, and a seam parser must never read it as
//! one. An unset pace adds nothing, so every existing label is unchanged.

use std::fmt;

/// The label's marker for a run's pace: `<label>@pace=<mode>`.
pub const PACE_LABEL_MARKER: &str = "@pace=";

/// How a board's guest clock is held against wall time. See [the module
/// docs](self); an unset pace is `Option::None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pace {
    /// 1×: never ahead of wall time.
    Realtime,
    /// As fast as possible: never paced.
    Max,
}

impl Pace {
    /// Every pace, in the order a help text lists them.
    pub const ALL: [Pace; 2] = [Pace::Realtime, Pace::Max];

    /// `realtime` / `max`: the label's word, and the command line's.
    pub fn as_str(self) -> &'static str {
        match self {
            Pace::Realtime => "realtime",
            Pace::Max => "max",
        }
    }

    /// What a configuration label carries for `pace`: `@pace=<mode>` when it
    /// was set, nothing when it was not.
    pub fn label_suffix(pace: Option<Pace>) -> String {
        pace.map_or_else(String::new, |p| format!("{PACE_LABEL_MARKER}{p}"))
    }
}

impl fmt::Display for Pace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_pace_adds_nothing_to_a_label() {
        assert_eq!(Pace::label_suffix(None), "");
        assert_eq!(Pace::label_suffix(Some(Pace::Realtime)), "@pace=realtime");
        assert_eq!(Pace::label_suffix(Some(Pace::Max)), "@pace=max");
    }

    #[test]
    fn a_pace_suffix_is_never_a_seam_atom() {
        // A seam atom is `+<seam>=<impl>`: the suffix carries no `+`, so a
        // parser that splits a label on `+` keeps it on the last piece, and
        // its `@` is a character no seam or implementation name holds.
        for pace in Pace::ALL {
            let suffix = Pace::label_suffix(Some(pace));
            assert!(!suffix.contains('+'), "{suffix}");
            assert!(suffix.starts_with('@'), "{suffix}");
        }
    }
}
