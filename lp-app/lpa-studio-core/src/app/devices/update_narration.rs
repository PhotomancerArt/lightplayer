//! What an update says in the card's terminal: one line per change of
//! state, in the update-states spike's terminal voice (lowercase, `·` between
//! clauses), and the line the walk reads its numbers from (DS11) — bytes,
//! seconds and KB/s per piece, and how long each reconnect took.
//!
//! Times are the controller's injected clock's (`now_ms`), never a core
//! clock. Nothing here ever sees a credential.

use lpa_devices::UpdateStageFacts;
use lpa_update::Decision;
use lpc_update::code_table::CHUNK;

use super::device_update_route::UpdateLink;
use super::device_update_version::UpdateVersion;

/// Who is being updated to what, for the opening line.
#[derive(Clone, Debug)]
pub(crate) struct NarrationNames {
    /// The board's version, as its manifest names it.
    pub board: UpdateVersion,
    /// The build the update puts on it.
    pub to: UpdateVersion,
    pub link: UpdateLink,
    /// The board already runs `to`'s core, on trial or mid-transfer: a heal
    /// is the tail of an update (finishing), not a restore.
    pub finishing: bool,
}

/// One piece moved (or read back): its length and the time spent moving it.
#[derive(Clone, Copy, Debug, Default)]
struct PieceStat {
    bytes: u32,
    active_ms: u64,
    /// The first and latest progress in the current leg.
    leg: Option<(u64, u64)>,
}

impl PieceStat {
    fn note(&mut self, now_ms: u64, total: u32) {
        self.bytes = total;
        self.leg = Some(match self.leg {
            Some((first, _)) => (first, now_ms),
            None => (now_ms, now_ms),
        });
    }

    /// The leg ended: bank its time.
    fn close_leg(&mut self) {
        if let Some((first, last)) = self.leg.take() {
            self.active_ms += last.saturating_sub(first);
        }
    }

    fn moved(&self) -> bool {
        self.bytes > 0
    }
}

/// The narration of one update run. See the module docs.
#[derive(Debug, Default)]
pub(crate) struct UpdateNarration {
    opened: bool,
    last_stage: Option<UpdateStageFacts>,
    /// The stage's latest progress, for a drop's "at 40%".
    last_progress: Option<(u32, u32)>,
    backup: PieceStat,
    core: PieceStat,
    engine: PieceStat,
    /// When the leg's link went down, and whether that was a drop mid-piece
    /// rather than the board's own reset.
    down: Option<(u64, bool)>,
}

impl UpdateNarration {
    /// The run's first decision: the opening line.
    pub fn decided(&mut self, decision: &Decision, names: &NarrationNames) -> Option<String> {
        let line = match decision {
            Decision::OfferUpdate { .. } => format!(
                "update {} → {} over {}",
                names.board.short(),
                names.to.short(),
                names.link.word()
            ),
            Decision::ContinueUpdate { .. } => {
                format!("board is half-way to {} · finishing it", names.to.short())
            }
            Decision::Heal { .. } if names.finishing => {
                format!("board is half-way to {} · finishing it", names.to.short())
            }
            Decision::Heal { .. } => format!(
                "board is missing its {} engine · restoring it",
                names.board.short()
            ),
            Decision::Reinstall { .. } => {
                format!(
                    "writing {} again over {}",
                    names.board.short(),
                    names.link.word()
                )
            }
            Decision::Busy { done, total } => format!(
                "another device is updating it{} · asking again every 3 s",
                percent_clause(*done, *total)
            ),
            // The rest end the run; the outcome says why.
            _ => return None,
        };
        // A Busy line may repeat while waiting; the opening line is once.
        if matches!(decision, Decision::Busy { .. }) {
            return Some(line);
        }
        if core::mem::replace(&mut self.opened, true) {
            return None;
        }
        Some(line)
    }

    /// A piece's progress. A line only when the stage changes to one worth
    /// naming (the bar carries the percent).
    pub fn progress(
        &mut self,
        now_ms: u64,
        stage: UpdateStageFacts,
        done: u32,
        total: u32,
    ) -> Option<String> {
        self.last_progress = Some((done, total));
        match stage {
            UpdateStageFacts::BackingUp => self.backup.note(now_ms, total),
            UpdateStageFacts::Updating => self.core.note(now_ms, total),
            UpdateStageFacts::Restoring | UpdateStageFacts::Finishing => {
                self.engine.note(now_ms, total)
            }
            UpdateStageFacts::Waiting => {}
        }
        if self.last_stage.replace(stage) == Some(stage) {
            return None;
        }
        match stage {
            UpdateStageFacts::BackingUp => {
                Some("backing up current firmware from the board".to_string())
            }
            _ => None,
        }
    }

    /// The backup came from this Studio's engine cache: no read-back.
    pub fn backup_cached(&self, board: &UpdateVersion) -> String {
        format!("backup of {} already here · no read-back", board.short())
    }

    /// An offer went to the board: whatever piece moved before it is
    /// whole (a backup read back, or nothing yet), so a reset from here on
    /// is the board doing its job.
    pub fn offered(&mut self) {
        self.last_progress = None;
    }

    /// The leg's link went down. A line only for a drop mid-piece (a reset
    /// after a whole piece is the board doing its job, and so is one before
    /// a piece moved a byte — the board's reset after its last commit
    /// reports the next stage at 0% first).
    pub fn link_down(&mut self, now_ms: u64) -> Option<String> {
        for piece in [&mut self.backup, &mut self.core, &mut self.engine] {
            piece.close_leg();
        }
        // A request names the offset it wants: the last one of a piece is
        // within a chunk of its end.
        let interrupted = self.last_progress.take().filter(|(done, total)| {
            *done > 0 && u64::from(*done) + u64::from(CHUNK) < u64::from(*total)
        });
        self.down = Some((now_ms, interrupted.is_some()));
        let (done, total) = interrupted?;
        Some(format!(
            "link dropped during update{}",
            percent_clause_at(done, total)
        ))
    }

    /// A new leg began on the board's link: how long the gap took.
    pub fn link_up(&mut self, now_ms: u64) -> Option<String> {
        let (at, mid_piece) = self.down.take()?;
        let took = format_secs(now_ms.saturating_sub(at));
        Some(match mid_piece {
            true => format!("reconnected in {took}"),
            false => format!("board reset · reconnected in {took}"),
        })
    }

    /// The run reached its goal: what moved, how much, how fast.
    pub fn rates(&mut self) -> Option<String> {
        let mut parts = Vec::new();
        for (name, piece) in [
            ("backup", &mut self.backup),
            ("core", &mut self.core),
            ("engine", &mut self.engine),
        ] {
            piece.close_leg();
            if piece.moved() {
                parts.push(rate_part(name, piece));
            }
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}

/// `core 1 160 KB in 52 s (22 KB/s)`; the rate is left out when the piece
/// moved in no measurable time.
fn rate_part(name: &str, piece: &PieceStat) -> String {
    let size = format_kb(piece.bytes);
    let took = format_secs(piece.active_ms);
    if piece.active_ms == 0 {
        return format!("{name} {size} in {took}");
    }
    let rate = u64::from(piece.bytes) / piece.active_ms; // bytes/ms = KB/s
    format!("{name} {size} in {took} ({rate} KB/s)")
}

/// ` · 40%` when a percent is known.
fn percent_clause(done: u32, total: u32) -> String {
    match percent(done, total) {
        Some(p) => format!(" · {p}%"),
        None => String::new(),
    }
}

/// ` at 40%` when a percent is known.
fn percent_clause_at(done: u32, total: u32) -> String {
    match percent(done, total) {
        Some(p) => format!(" at {p}%"),
        None => String::new(),
    }
}

fn percent(done: u32, total: u32) -> Option<u64> {
    (total > 0).then(|| u64::from(done.min(total)) * 100 / u64::from(total))
}

/// Kilobytes (1000 bytes), grouped by thousands with a space:
/// `1 160 KB`.
pub(crate) fn format_kb(bytes: u32) -> String {
    let kb = (u64::from(bytes) + 500) / 1000;
    let digits = kb.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(digit);
    }
    format!("{out} KB")
}

/// Seconds: one decimal under ten (`1.2 s`), whole above (`52 s`).
pub(crate) fn format_secs(ms: u64) -> String {
    if ms < 10_000 {
        format!("{}.{} s", ms / 1000, (ms % 1000) / 100)
    } else {
        format!("{} s", (ms + 500) / 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_times_read_the_way_the_walk_quotes_them() {
        assert_eq!(format_kb(1_160_000), "1 160 KB");
        assert_eq!(format_kb(1_822_400), "1 822 KB");
        assert_eq!(format_kb(24_876), "25 KB");
        assert_eq!(format_secs(1_200), "1.2 s");
        assert_eq!(format_secs(52_300), "52 s");
        assert_eq!(format_secs(0), "0.0 s");
    }

    #[test]
    fn a_whole_update_narrates_its_pieces_and_its_reset() {
        let names = NarrationNames {
            board: UpdateVersion::with_build_id("2026.10.03-1", "2026.10.03-1+aaaaaaaaaaaa"),
            to: UpdateVersion::with_build_id("2026.10.05-2", "2026.10.05-2+bbbbbbbbbbbb"),
            link: UpdateLink::Bluetooth,
            finishing: false,
        };
        let mut n = UpdateNarration::default();
        let offer = Decision::OfferUpdate {
            from: "a".into(),
            to: "b".into(),
        };
        assert_eq!(
            n.decided(&offer, &names).as_deref(),
            Some("update 2026.10.03-1 → 2026.10.05-2 over Bluetooth")
        );
        assert_eq!(n.decided(&offer, &names), None, "the opening line is once");
        assert_eq!(
            n.progress(0, UpdateStageFacts::BackingUp, 0, 1_822_000)
                .as_deref(),
            Some("backing up current firmware from the board")
        );
        n.progress(81_000, UpdateStageFacts::BackingUp, 1_818_000, 1_822_000);
        n.offered();
        assert_eq!(n.link_down(81_500), None, "a reset after the offer");
        assert_eq!(
            n.link_up(82_700).as_deref(),
            Some("board reset · reconnected in 1.2 s")
        );
        n.progress(83_000, UpdateStageFacts::Updating, 0, 1_160_000);
        n.progress(135_000, UpdateStageFacts::Updating, 400_000, 1_160_000);
        assert_eq!(
            n.link_down(135_100).as_deref(),
            Some("link dropped during update at 34%")
        );
        assert_eq!(n.link_up(136_000).as_deref(), Some("reconnected in 0.9 s"));
        assert_eq!(
            n.rates().as_deref(),
            Some("backup 1 822 KB in 81 s (22 KB/s) · core 1 160 KB in 52 s (22 KB/s)")
        );
    }

    /// A reset before a piece moved a byte is the board's own: its last
    /// commit's reset reports the next stage at 0% first (seen in the
    /// emulator walk), and that is no drop.
    #[test]
    fn a_reset_at_zero_percent_is_the_boards_not_a_drop() {
        let mut n = UpdateNarration::default();
        n.progress(1_000, UpdateStageFacts::Finishing, 0, 1_822_000);
        assert_eq!(n.link_down(1_500), None);
        assert_eq!(
            n.link_up(4_500).as_deref(),
            Some("board reset · reconnected in 3.0 s")
        );
    }
}
