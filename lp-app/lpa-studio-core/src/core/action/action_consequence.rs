//! [`ActionConsequence`]: what pressing an action costs the user.

use crate::ActionConfirmation;

/// What pressing an action costs the user: the one semantic flag that
/// decides both how the UI asks (D7) and whether the app agent may press
/// it or must hand the real control to the user.
///
/// One look per level, whichever component draws the button (Q7):
/// `Routine` is plain, `Undoable` wears the error tint and dispatches on one
/// click, and `Lasting` wears the error tint and arms on the first click.
/// There is no dialog at any level.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ActionConsequence {
    /// Nothing is lost. Plain button; the agent presses it.
    #[default]
    Routine,
    /// Removes something the user can still get back in Studio (Revert
    /// brings a removed node back until they save). Error tint, one
    /// click; the agent presses it and says what it did.
    Undoable,
    /// Gone for good, from Studio or from a board. Error tint, two-click
    /// arm; the agent hands the real control to the user. Carries the
    /// words that say what is lost.
    Lasting(ActionConfirmation),
}

impl ActionConsequence {
    /// Nothing is lost.
    pub fn is_routine(&self) -> bool {
        matches!(self, Self::Routine)
    }

    /// Whether the button wears the error tint: every level that takes
    /// something away.
    pub fn wears_error_tint(&self) -> bool {
        !self.is_routine()
    }

    /// Whether the button arms on its first click and acts on the second.
    pub fn arms(&self) -> bool {
        matches!(self, Self::Lasting(_))
    }

    /// The words that say what is lost, for a `Lasting` action.
    pub fn copy(&self) -> Option<&ActionConfirmation> {
        match self {
            Self::Lasting(copy) => Some(copy),
            Self::Routine | Self::Undoable => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_level_reads_its_treatment() {
        let routine = ActionConsequence::Routine;
        assert!(routine.is_routine());
        assert!(!routine.wears_error_tint());
        assert!(!routine.arms());
        assert!(routine.copy().is_none());

        let undoable = ActionConsequence::Undoable;
        assert!(!undoable.is_routine());
        assert!(undoable.wears_error_tint());
        assert!(!undoable.arms());
        assert!(undoable.copy().is_none());

        let copy = ActionConfirmation::new("Forget?", "It goes.", "forget");
        let lasting = ActionConsequence::Lasting(copy.clone());
        assert!(!lasting.is_routine());
        assert!(lasting.wears_error_tint());
        assert!(lasting.arms());
        assert_eq!(lasting.copy(), Some(&copy));
    }

    #[test]
    fn routine_is_the_default() {
        assert_eq!(ActionConsequence::default(), ActionConsequence::Routine);
    }
}
