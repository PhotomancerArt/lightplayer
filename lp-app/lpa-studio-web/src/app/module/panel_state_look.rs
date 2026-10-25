//! What a panel control's state looks like, wherever a panel draws it (the
//! editor's and the play page's [`super::ModulePanelControl`], the board
//! card's [`super::PanelCell`] and [`super::PanelStrip`]): the label's and the
//! value's colour family, and the walk hook that names the state.
//!
//! Three states, three families (`docs/design/panel.md` P-Q2), none of them
//! green: the quiet default, the bound violet while the control follows what
//! drives it, the engaged gold while a panel write holds it.

use lpa_studio_core::UiPanelControlState;

/// The walk hook's name for a panel state (`data-panel-state`): what a walk
/// reads to tell the board's held control from one it only follows.
pub(crate) fn panel_state_hook(state: UiPanelControlState) -> &'static str {
    match state {
        UiPanelControlState::ReadDefault => "read-default",
        UiPanelControlState::ReadFollowing => "read-following",
        UiPanelControlState::Engaged => "engaged",
    }
}

/// Label color per panel state. Three families, none of them green.
pub(crate) fn panel_state_label_class(state: UiPanelControlState) -> &'static str {
    match state {
        UiPanelControlState::ReadDefault => "tw:text-subtle-foreground",
        UiPanelControlState::ReadFollowing => "tw:text-status-bound-foreground",
        UiPanelControlState::Engaged => "tw:text-status-engaged-foreground",
    }
}

/// Readout color per panel state: the held value leads in engaged gold, a
/// followed value in violet, an untouched default stays quiet.
pub(crate) fn panel_state_readout_class(state: UiPanelControlState) -> &'static str {
    match state {
        UiPanelControlState::ReadDefault => "tw:text-dim-foreground",
        UiPanelControlState::ReadFollowing => "tw:text-status-bound-foreground",
        UiPanelControlState::Engaged => "tw:text-status-engaged-foreground",
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::UiPanelControlState;

    use super::{panel_state_hook, panel_state_label_class, panel_state_readout_class};

    #[test]
    fn the_three_panel_states_wear_three_different_families() {
        let families: Vec<&str> = [
            UiPanelControlState::ReadDefault,
            UiPanelControlState::ReadFollowing,
            UiPanelControlState::Engaged,
        ]
        .into_iter()
        .map(panel_state_label_class)
        .collect();

        // P-Q2's requirement, pinned: three visibly distinct states.
        assert_eq!(families.len(), 3);
        assert_ne!(families[0], families[1]);
        assert_ne!(families[1], families[2]);
        assert_ne!(families[0], families[2]);
        // Engaged must NOT reuse the bound-violet family (P6).
        assert!(families[1].contains("bound"));
        assert!(!families[2].contains("bound"));
        assert!(families[2].contains("engaged"));
        // And green stays valid-only, everywhere.
        for family in families.iter().chain(
            [
                panel_state_readout_class(UiPanelControlState::ReadDefault),
                panel_state_readout_class(UiPanelControlState::ReadFollowing),
                panel_state_readout_class(UiPanelControlState::Engaged),
            ]
            .iter(),
        ) {
            assert!(!family.contains("good"), "green is valid-only: {family}");
        }
    }

    /// The walk hook names each state once.
    #[test]
    fn each_state_has_its_own_walk_hook() {
        assert_eq!(
            panel_state_hook(UiPanelControlState::ReadDefault),
            "read-default"
        );
        assert_eq!(
            panel_state_hook(UiPanelControlState::ReadFollowing),
            "read-following"
        );
        assert_eq!(panel_state_hook(UiPanelControlState::Engaged), "engaged");
    }
}
