//! What the Flash activity learns about the board in its bootloader session
//! — the chip, the build it gets, the board model when it can be told, and
//! what the board ran before — as the model sees it.
//!
//! The install used to ask "which board is this?" before anything else,
//! because the build was picked before the port was opened (plan
//! `lp2025/2026-10-28-0025-install-without-board-pick`). Now the action
//! carries a [`FlashBuildChoice`] and the inspection's session answers it by
//! the chip its SYNC names; the effects layer — which owns the board catalog
//! — matches the board and reports a [`BoardIdentity`] marker just before
//! the layout verdict. The model treats every field as data.

use serde::{Deserialize, Serialize};

/// Which build a Flash writes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FlashBuildChoice {
    /// This build (a release the user picked, or a board whose chip and
    /// build were certain before the press).
    Fixed(String),
    /// Whichever build the chip the bootloader names gets, from this
    /// table; a chip it lacks gets nothing, and the install stops there.
    ByChip(Vec<ChipBuildRow>),
}

/// One row of a [`FlashBuildChoice::ByChip`] table.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChipBuildRow {
    pub chip: String,
    pub build_id: String,
}

impl FlashBuildChoice {
    /// The build when it is known before the session.
    pub fn fixed(&self) -> Option<&str> {
        match self {
            Self::Fixed(build_id) => Some(build_id),
            Self::ByChip(_) => None,
        }
    }
}

/// What a board's flash held before the install.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Resident {
    /// Nothing: an erased chip.
    Blank,
    /// LightPlayer, any version: the install keeps its files.
    LightPlayer,
    /// Somebody else's firmware, as it names itself.
    Foreign {
        project_name: Option<String>,
        version: Option<String>,
    },
    /// Something is there and does not say what.
    Unrecognized,
}

impl Resident {
    /// Whether installing replaces somebody else's firmware — the one
    /// question an install asks before writing.
    pub fn is_replaced_by_install(&self) -> bool {
        matches!(self, Self::Foreign { .. } | Self::Unrecognized)
    }
}

/// The inspection's identification of the board.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BoardIdentity {
    /// The chip id the bootloader named (`esp32c6`), when it resolved.
    pub chip: Option<String>,
    /// The build the session chose; `None` when LightPlayer has none for
    /// this chip.
    pub build_id: Option<String>,
    /// The board model, when exactly one catalog board fits.
    pub board_id: Option<String>,
    /// Every catalog board that fits, for asking afterwards.
    pub candidates: Vec<String>,
    /// What the board ran before.
    pub resident: Resident,
}

impl BoardIdentity {
    /// One line for the board's terminal: what the session found.
    pub fn terminal_line(&self) -> String {
        let chip = self.chip.as_deref().unwrap_or("an unknown chip");
        let build = match &self.build_id {
            Some(build) => format!("firmware {build}"),
            None => "no LightPlayer firmware for it".to_string(),
        };
        let board = match (&self.board_id, self.candidates.len()) {
            (Some(board), _) => format!("board {board}"),
            (None, 0) => "no catalog board fits".to_string(),
            (None, n) => format!("{n} boards fit"),
        };
        let resident = match &self.resident {
            Resident::Blank => "blank".to_string(),
            Resident::LightPlayer => "running LightPlayer".to_string(),
            Resident::Foreign {
                project_name,
                version,
            } => {
                let named = [project_name.as_deref(), version.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                match named.is_empty() {
                    true => "running other firmware".to_string(),
                    false => format!("running {named}"),
                }
            }
            Resident::Unrecognized => "running something that does not say what".to_string(),
        };
        format!("identified: {chip} · {build} · {board} · {resident}")
    }
}

/// The replace question, as the card shows it while the install waits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FlashReplaceView {
    pub resident: Resident,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_somebody_elses_firmware_is_replaced() {
        assert!(!Resident::Blank.is_replaced_by_install());
        assert!(!Resident::LightPlayer.is_replaced_by_install());
        assert!(Resident::Unrecognized.is_replaced_by_install());
        assert!(
            Resident::Foreign {
                project_name: None,
                version: None
            }
            .is_replaced_by_install()
        );
    }

    #[test]
    fn a_fixed_build_is_known_before_the_session() {
        assert_eq!(FlashBuildChoice::Fixed("b".into()).fixed(), Some("b"));
        assert_eq!(FlashBuildChoice::ByChip(Vec::new()).fixed(), None);
    }
}
