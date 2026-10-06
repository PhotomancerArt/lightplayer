//! Which emulator seams a tab-hosted board asks for, and the one journal line
//! that says what came of it (ADR `docs/adr/2026-10-05-emulator-seams.md`).
//!
//! Pure, and declared outside the wasm gate so its tests run natively; the
//! bridge reads the worker's report into [`EmuSeamsInfo`] and the link hands
//! [`SeamNoteTracker`]'s line to the journal as a `WireNote`.
//!
//! **End users only.** The emulated boards a user adds on Studio's Devices
//! page ask for [`END_USER_SEAMS`] — the LED performance seam, which keeps
//! frames, fps and heap and costs the browser about a fifth less. They ask
//! **softly** (`seams_prefer=`), because a saved board can hold firmware older
//! than this Studio, and a soft request on such a board boots seam-free and
//! says why rather than refusing to boot. Nothing else asks: the dev path
//! (`?emu=tab`, `?emu=ws://…`), `emu serve` and every walk run today's
//! machine. The dev flag `?seams=<atoms|none>` replaces the end-user choice,
//! for Devices-page boards only.

use std::cell::RefCell;

/// What a Devices-page emulated board asks for unless `?seams=` says
/// otherwise.
pub const END_USER_SEAMS: &str = "led=fast";

thread_local! {
    /// `?seams=`, when the page carried one (`dev_url_flags.rs`).
    static OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Dev-only (Studio's `?seams=<atoms|none>`): replace the end-user choice
/// for every Devices-page board opened after the call. `None` restores
/// [`END_USER_SEAMS`].
pub fn set_end_user_seams_override(seams: Option<String>) {
    OVERRIDE.with(|slot| *slot.borrow_mut() = seams);
}

/// What a Devices-page board asks for: the dev override, else
/// [`END_USER_SEAMS`].
pub fn end_user_seams() -> String {
    OVERRIDE
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(|| END_USER_SEAMS.to_string())
}

/// A board's ask: what its options said (`Some`), else the end-user choice.
pub fn resolve_seams(asked: Option<&str>) -> String {
    asked.map_or_else(end_user_seams, str::to_string)
}

/// The board config line for `seams`: `seams_prefer=<atoms>`, or nothing for
/// `none` (a seam-free board — today's machine).
pub fn seams_cfg_line(seams: &str) -> Option<String> {
    let seams = seams.trim();
    if seams.is_empty() || seams == "none" {
        return None;
    }
    Some(format!("seams_prefer={seams}"))
}

/// What the worker last said about a board's seams (`emu_seams_info`), with
/// what the board asked for and how many times it has restarted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EmuSeamsInfo {
    /// The board's ask (`led=fast`, `none`).
    pub asked: String,
    /// Restarts so far: a chip start is a new chance to say something.
    pub reboots: u64,
    /// The engaged atoms this chip start.
    pub engaged: Vec<String>,
    /// Why a soft request engaged nothing this chip start, if it did not.
    pub none_why: Option<String>,
}

impl EmuSeamsInfo {
    /// The journal line this report deserves, in plain words, or `None`
    /// while there is nothing to say yet (a ROM-up board resolves its seams
    /// once its app runs).
    pub fn journal_line(&self) -> Option<String> {
        if self.asked.trim() == "none" {
            return Some("emu: LED fast mode off (?seams=none)".to_string());
        }
        if !self.engaged.is_empty() {
            let atoms = self.engaged.join("+");
            return Some(match atoms.as_str() {
                "led=fast" => "emu: LED fast mode on (led=fast)".to_string(),
                _ => format!("emu: emulator seams on ({atoms})"),
            });
        }
        let why = self.none_why.as_deref()?;
        Some(if why.starts_with("no seam table in the image") {
            "emu: LED fast mode off — this board's firmware is older than this Studio; Update \
             firmware to turn it on"
                .to_string()
        } else if why.contains("different seam declarations") {
            "emu: LED fast mode off — this board's firmware was built for a different Studio; \
             Update firmware to turn it on"
                .to_string()
        } else {
            format!("emu: LED fast mode off — {why}")
        })
    }

    /// What makes two reports the same news: the chip start and what it said.
    fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.asked,
            self.reboots,
            self.engaged.join("+"),
            self.none_why.as_deref().unwrap_or("")
        )
    }
}

/// One line per chip start, never the same news twice.
#[derive(Clone, Debug, Default)]
pub struct SeamNoteTracker {
    last: Option<String>,
}

impl SeamNoteTracker {
    /// The line to journal for `info`, if it is news.
    pub fn note(&mut self, info: &EmuSeamsInfo) -> Option<String> {
        let line = info.journal_line()?;
        // `?seams=none` is said once for the board's life, not per start.
        let key = if info.asked.trim() == "none" {
            "none".to_string()
        } else {
            info.key()
        };
        if self.last.as_deref() == Some(key.as_str()) {
            return None;
        }
        self.last = Some(key);
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_devices_page_board_asks_for_led_fast_softly_unless_overridden() {
        set_end_user_seams_override(None);
        assert_eq!(resolve_seams(None), "led=fast");
        assert_eq!(
            seams_cfg_line(&resolve_seams(None)).as_deref(),
            Some("seams_prefer=led=fast")
        );
        set_end_user_seams_override(Some("none".into()));
        assert_eq!(resolve_seams(None), "none");
        assert_eq!(seams_cfg_line("none"), None, "a seam-free board");
        set_end_user_seams_override(None);
        assert_eq!(resolve_seams(Some("none")), "none", "an explicit ask wins");
        assert_eq!(
            seams_cfg_line("led=fast+x=y").as_deref(),
            Some("seams_prefer=led=fast+x=y")
        );
    }

    #[test]
    fn the_journal_line_reads_plainly_for_each_outcome() {
        let on = EmuSeamsInfo {
            asked: "led=fast".into(),
            engaged: vec!["led=fast".into()],
            ..EmuSeamsInfo::default()
        };
        assert_eq!(
            on.journal_line().as_deref(),
            Some("emu: LED fast mode on (led=fast)")
        );
        let old = EmuSeamsInfo {
            asked: "led=fast".into(),
            none_why: Some("no seam table in the image (emulator abi 1234)".into()),
            ..EmuSeamsInfo::default()
        };
        assert!(
            old.journal_line()
                .unwrap()
                .contains("older than this Studio; Update firmware")
        );
        let other = EmuSeamsInfo {
            none_why: Some("built from different seam declarations".into()),
            ..old.clone()
        };
        assert!(other.journal_line().unwrap().contains("different Studio"));
        let two = EmuSeamsInfo {
            none_why: Some("no seam table is live: none of the 2 in flash".into()),
            ..old.clone()
        };
        assert_eq!(
            two.journal_line().as_deref(),
            Some("emu: LED fast mode off — no seam table is live: none of the 2 in flash"),
            "only a missing table is old firmware"
        );
        let forced = EmuSeamsInfo {
            asked: "none".into(),
            ..EmuSeamsInfo::default()
        };
        assert_eq!(
            forced.journal_line().as_deref(),
            Some("emu: LED fast mode off (?seams=none)")
        );
        let waiting = EmuSeamsInfo {
            asked: "led=fast".into(),
            ..EmuSeamsInfo::default()
        };
        assert_eq!(waiting.journal_line(), None, "nothing until the app runs");
    }

    #[test]
    fn one_line_per_chip_start() {
        let mut tracker = SeamNoteTracker::default();
        let blank = EmuSeamsInfo {
            asked: "led=fast".into(),
            none_why: Some("no seam table in the image".into()),
            ..EmuSeamsInfo::default()
        };
        assert!(tracker.note(&blank).is_some(), "the blank board says why");
        assert!(tracker.note(&blank).is_none(), "once");
        // Update firmware: a reset, then the app engages the seam.
        let resolving = EmuSeamsInfo {
            reboots: 1,
            none_why: None,
            ..blank.clone()
        };
        assert!(tracker.note(&resolving).is_none(), "nothing yet");
        let engaged = EmuSeamsInfo {
            engaged: vec!["led=fast".into()],
            ..resolving
        };
        assert_eq!(
            tracker.note(&engaged).as_deref(),
            Some("emu: LED fast mode on (led=fast)")
        );
        assert!(tracker.note(&engaged).is_none());
        // A plain reboot of the same image: a new chip start, said again.
        let again = EmuSeamsInfo {
            reboots: 2,
            ..engaged
        };
        assert!(tracker.note(&again).is_some());

        let mut forced = SeamNoteTracker::default();
        let none = EmuSeamsInfo {
            asked: "none".into(),
            ..EmuSeamsInfo::default()
        };
        assert!(forced.note(&none).is_some());
        let none_rebooted = EmuSeamsInfo { reboots: 3, ..none };
        assert!(forced.note(&none_rebooted).is_none(), "said once per board");
    }
}
