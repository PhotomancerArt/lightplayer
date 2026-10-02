//! [`PaletteCursor`]: which row the palette's keys are on, and whether
//! that row is armed — the palette's keyboard grammar as a plain value, so
//! it is testable without mounting.
//!
//! Both are held by [`OfferPath`], never by index: the view republishes the
//! tree while the palette is open (a save lands, a node goes clean), and a
//! cursor keyed on an index would slide onto a different verb under the
//! user's finger. A highlight whose row has gone falls back to the first
//! row Enter can press.

use lpa_studio_core::{OfferPath, UiAction, UiOffer};

/// A key the palette's input answers. Everything else is typing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaletteKey {
    Up,
    Down,
    Enter,
    Escape,
}

/// What a key asks the palette to do.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PaletteOutcome {
    /// Nothing beyond the cursor moving (or nothing at all).
    Stay,
    /// The highlighted Lasting row armed: the next Enter presses it. The
    /// palette stays open and starts the arm's window.
    Armed,
    /// Press this action, then close.
    Press(UiAction),
    /// Close without pressing.
    Close,
}

/// The highlighted row and the armed row, by path.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PaletteCursor {
    highlight: Option<OfferPath>,
    armed: Option<OfferPath>,
}

impl PaletteCursor {
    /// The row the keys are on among `rows`: the held highlight while it is
    /// still listed and pressable, else the first row Enter can press.
    pub(crate) fn highlighted(&self, rows: &[&UiOffer]) -> Option<usize> {
        self.highlight
            .as_ref()
            .and_then(|path| {
                rows.iter()
                    .position(|row| &row.path == path && row.is_enabled())
            })
            .or_else(|| rows.iter().position(|row| row.is_enabled()))
    }

    /// Whether `path` is the armed row.
    pub(crate) fn is_armed(&self, path: &OfferPath) -> bool {
        self.armed.as_ref() == Some(path)
    }

    /// Answer one key over the rows on screen.
    pub(crate) fn key(&mut self, key: PaletteKey, rows: &[&UiOffer]) -> PaletteOutcome {
        match key {
            PaletteKey::Escape => PaletteOutcome::Close,
            PaletteKey::Up => {
                self.step(rows, false);
                PaletteOutcome::Stay
            }
            PaletteKey::Down => {
                self.step(rows, true);
                PaletteOutcome::Stay
            }
            PaletteKey::Enter => self.enter(rows),
        }
    }

    /// The pointer moved onto `row`. A disabled row is not a stop
    /// (Enter would skip it), and moving off an armed row stands it down,
    /// like a button losing focus.
    pub(crate) fn hover(&mut self, row: &UiOffer) {
        if !row.is_enabled() || self.highlight.as_ref() == Some(&row.path) {
            return;
        }
        self.highlight = Some(row.path.clone());
        self.armed = None;
    }

    /// The query changed: back to the first pressable row, nothing armed.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// The arm's window ran out.
    pub(crate) fn disarm(&mut self) {
        self.armed = None;
    }

    /// Move to the next (or previous) pressable row, wrapping; disabled rows
    /// are passed over. Moving stands an armed row down.
    fn step(&mut self, rows: &[&UiOffer], forward: bool) {
        let pressable: Vec<usize> = (0..rows.len())
            .filter(|&at| rows[at].is_enabled())
            .collect();
        if pressable.is_empty() {
            return;
        }
        let next = match self
            .highlighted(rows)
            .and_then(|at| pressable.iter().position(|&row| row == at))
        {
            Some(slot) if forward => pressable[(slot + 1) % pressable.len()],
            Some(slot) => pressable[(slot + pressable.len() - 1) % pressable.len()],
            None => pressable[0],
        };
        self.highlight = Some(rows[next].path.clone());
        self.armed = None;
    }

    /// Enter: press the highlighted row — or, for a Lasting one not yet
    /// armed, arm it. No pressable row, nothing happens.
    fn enter(&mut self, rows: &[&UiOffer]) -> PaletteOutcome {
        let Some(row) = self.highlighted(rows).map(|at| rows[at]) else {
            return PaletteOutcome::Stay;
        };
        self.highlight = Some(row.path.clone());
        if row.consequence().arms() && !self.is_armed(&row.path) {
            self.armed = Some(row.path.clone());
            return PaletteOutcome::Armed;
        }
        self.armed = None;
        PaletteOutcome::Press(row.action.clone())
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ActionConfirmation, ControllerId, ProjectOp};

    use super::*;

    #[test]
    fn the_first_pressable_row_starts_highlighted() {
        let rows = rows();
        let cursor = PaletteCursor::default();

        assert_eq!(cursor.highlighted(&refs(&rows)), Some(0));

        let disabled_first = [disabled("project/save"), routine("project/revert")];
        assert_eq!(
            cursor.highlighted(&refs(&disabled_first)),
            Some(1),
            "a disabled row is never where the keys start"
        );
    }

    #[test]
    fn arrows_move_wrap_and_pass_over_disabled_rows() {
        let rows = rows();
        let rows = refs(&rows);
        let mut cursor = PaletteCursor::default();

        assert_eq!(cursor.key(PaletteKey::Down, &rows), PaletteOutcome::Stay);
        assert_eq!(cursor.highlighted(&rows), Some(1));
        cursor.key(PaletteKey::Down, &rows);
        assert_eq!(
            cursor.highlighted(&rows),
            Some(3),
            "row 2 is disabled: the arrow passes over it"
        );
        cursor.key(PaletteKey::Down, &rows);
        assert_eq!(cursor.highlighted(&rows), Some(0), "wraps to the top");
        cursor.key(PaletteKey::Up, &rows);
        assert_eq!(cursor.highlighted(&rows), Some(3), "and to the bottom");
    }

    #[test]
    fn enter_presses_a_routine_row_and_escape_closes() {
        let rows = rows();
        let rows = refs(&rows);
        let mut cursor = PaletteCursor::default();

        assert_eq!(
            cursor.key(PaletteKey::Enter, &rows),
            PaletteOutcome::Press(rows[0].action.clone())
        );
        assert_eq!(cursor.key(PaletteKey::Escape, &rows), PaletteOutcome::Close);
    }

    #[test]
    fn a_lasting_row_arms_on_the_first_enter_and_presses_on_the_second() {
        let rows = rows();
        let rows = refs(&rows);
        let mut cursor = PaletteCursor::default();
        cursor.key(PaletteKey::Down, &rows);

        assert_eq!(cursor.key(PaletteKey::Enter, &rows), PaletteOutcome::Armed);
        assert!(cursor.is_armed(&rows[1].path));
        assert_eq!(
            cursor.key(PaletteKey::Enter, &rows),
            PaletteOutcome::Press(rows[1].action.clone())
        );
        assert!(!cursor.is_armed(&rows[1].path), "pressing stands it down");
    }

    #[test]
    fn moving_off_an_armed_row_stands_it_down() {
        let rows = rows();
        let rows = refs(&rows);
        let mut cursor = PaletteCursor::default();
        cursor.key(PaletteKey::Down, &rows);
        cursor.key(PaletteKey::Enter, &rows);

        cursor.key(PaletteKey::Down, &rows);
        cursor.key(PaletteKey::Up, &rows);
        assert_eq!(cursor.highlighted(&rows), Some(1));
        assert_eq!(
            cursor.key(PaletteKey::Enter, &rows),
            PaletteOutcome::Armed,
            "back on the row, it has to arm again"
        );

        cursor.hover(rows[0]);
        assert!(
            !cursor.is_armed(&rows[1].path),
            "the pointer moving off counts too"
        );

        cursor.key(PaletteKey::Down, &rows);
        cursor.key(PaletteKey::Enter, &rows);
        cursor.disarm();
        assert_eq!(
            cursor.key(PaletteKey::Enter, &rows),
            PaletteOutcome::Armed,
            "the window running out counts too"
        );
    }

    #[test]
    fn hovering_a_disabled_row_leaves_the_highlight() {
        let rows = rows();
        let rows = refs(&rows);
        let mut cursor = PaletteCursor::default();

        cursor.hover(rows[2]);
        assert_eq!(cursor.highlighted(&rows), Some(0));
        cursor.hover(rows[3]);
        assert_eq!(cursor.highlighted(&rows), Some(3));
    }

    #[test]
    fn enter_with_nothing_pressable_does_nothing() {
        let rows = [disabled("project/save")];
        let mut cursor = PaletteCursor::default();

        assert_eq!(
            cursor.key(PaletteKey::Enter, &refs(&rows)),
            PaletteOutcome::Stay
        );
        assert_eq!(cursor.key(PaletteKey::Enter, &[]), PaletteOutcome::Stay);
        assert_eq!(cursor.key(PaletteKey::Down, &[]), PaletteOutcome::Stay);
    }

    #[test]
    fn a_highlight_whose_row_went_away_falls_back_to_the_first() {
        let rows = rows();
        let mut cursor = PaletteCursor::default();
        cursor.key(PaletteKey::Up, &refs(&rows));
        assert_eq!(cursor.highlighted(&refs(&rows)), Some(3));

        let fewer = [routine("project/save"), lasting("project/revert")];
        assert_eq!(cursor.highlighted(&refs(&fewer)), Some(0));

        cursor.reset();
        assert_eq!(cursor.highlighted(&refs(&rows)), Some(0));
    }

    /// Save (routine), Revert to saved (Lasting), a disabled row, and a
    /// node revert (routine).
    fn rows() -> [UiOffer; 4] {
        [
            routine("project/save"),
            lasting("project/revert"),
            disabled("project/demo.module/remove"),
            routine("project/demo.module/revert"),
        ]
    }

    fn refs(rows: &[UiOffer]) -> Vec<&UiOffer> {
        rows.iter().collect()
    }

    fn routine(path: &str) -> UiOffer {
        UiOffer::new(
            OfferPath::parse(path).unwrap(),
            "save",
            UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay),
        )
    }

    fn lasting(path: &str) -> UiOffer {
        let mut offer = routine(path);
        offer.action = offer.action.lasting(ActionConfirmation::new(
            "Revert to saved?",
            "Every unsaved edit is discarded.",
            "revert",
        ));
        offer
    }

    fn disabled(path: &str) -> UiOffer {
        let mut offer = routine(path);
        offer.action = offer.action.disabled("nothing to remove");
        offer
    }
}
