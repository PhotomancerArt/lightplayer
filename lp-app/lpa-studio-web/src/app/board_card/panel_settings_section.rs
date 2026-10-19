//! The connected panel's settings, a section of the status corner's
//! details ([`UiDetailPanel::PanelSettings`], the card panel pass): the
//! panel holds only controls, and the ⋯ holds the two settings that sit
//! beside them on the play page.
//!
//! - **Held controls**: how many a panel writer holds, and Reset, which lets
//!   go of every one under the root's scope (descending into nested groups)
//!   so the project controls them again. Offered only while one is held.
//! - **Auto-save**: whether held values come back after a restart; at the
//!   edit tier only (core leaves it out below it).
//!
//! Both send their panel commands through the panel's own gesture seam
//! ([`panel_gesture_actions`]), as the play page's chrome does: the same
//! two actions, built where they always were (making them offers is the
//! agentic UI roadmap's).
//!
//! Walk hooks: `data-panel-reset` on Reset, `data-panel-auto-save` on the
//! switch.
//!
//! [`UiDetailPanel::PanelSettings`]: lpa_studio_core::UiDetailPanel::PanelSettings

use dioxus::prelude::*;
use lpa_studio_core::UiAction;

use crate::app::home::access_fields::{Switch, TEXT_BUTTON_CLASS};
use crate::app::module::{PanelGesture, panel_gesture_actions};
use crate::base::{StudioIcon, StudioIconName};

/// See the module doc.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PanelSettingsSection(
    target: Option<lpc_wire::WireScopeRef>,
    held: usize,
    auto_save: Option<bool>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let on_panel = panel_gesture_actions(on_action);
    let reset = target.filter(|_| held > 0);
    rsx! {
        section { class: SECTION_CLASS,
            if target.is_some() {
                div { class: ROW_CLASS,
                    span { class: ICON_CLASS, aria_hidden: "true",
                        StudioIcon { name: StudioIconName::Revert, size: 14 }
                    }
                    span { class: "tw:grid tw:min-w-0 tw:flex-1",
                        "Held controls"
                        span { class: SUB_CLASS, "{held_words(held)}" }
                    }
                    if let Some(scope) = reset {
                        button {
                            class: TEXT_BUTTON_CLASS,
                            r#type: "button",
                            "data-panel-reset": "",
                            title: "Let go of every held control, so the project controls them again",
                            onclick: move |_| on_panel.call(PanelGesture::ClearScope { scope }),
                            "Reset"
                        }
                    }
                }
            }
            if let Some(on) = auto_save {
                div { class: ROW_CLASS, "data-panel-auto-save": "{on}",
                    span { class: ICON_CLASS, aria_hidden: "true",
                        StudioIcon { name: StudioIconName::Save, size: 14 }
                    }
                    span { class: "tw:grid tw:min-w-0 tw:flex-1",
                        "Auto-save"
                        span { class: SUB_CLASS, "{auto_save_words(on)}" }
                    }
                    Switch {
                        on,
                        label: "Auto-save held controls".to_string(),
                        on_toggle: move |on| on_panel.call(PanelGesture::SetAutoSave(on)),
                    }
                }
            }
        }
    }
}

/// How many controls are held, in words.
fn held_words(held: usize) -> String {
    match held {
        0 => "None held".to_string(),
        1 => "1 held".to_string(),
        held => format!("{held} held"),
    }
}

/// What auto-save does, said for its state.
fn auto_save_words(on: bool) -> &'static str {
    match on {
        true => "Held values will come back after a restart.",
        false => "Held values will be lost on a restart.",
    }
}

/// A section of the details card: its divider and padding, no frame.
const SECTION_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-2 tw:border-0 tw:border-t tw:border-solid tw:border-border-muted tw:px-3 tw:py-2 tw:first:border-t-0";

/// A setting's row: its icon, its name over its state, its control.
const ROW_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2.5 tw:text-[13px] tw:font-semibold tw:text-strong-foreground";

const ICON_CLASS: &str = "tw:inline-flex tw:flex-none tw:text-subtle-foreground";

/// The state under a setting's name.
const SUB_CLASS: &str = "tw:text-[11px] tw:font-medium tw:text-dim-foreground";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::board_card::card_test_fixtures::render;

    /// Reset is offered only while a control is held, and only with a
    /// scope to clear; auto-save only when core gives it (the edit tier).
    #[test]
    fn reset_while_held_and_auto_save_at_the_edit_tier() {
        let scope = lpc_wire::WireScopeRef::Module {
            owner: lpc_model::NodeId::new(1),
        };
        let html = render_section(Some(scope), 2, Some(true));
        assert!(html.contains("data-panel-reset"), "{html}");
        assert!(html.contains("2 held"), "{html}");
        assert!(html.contains(r#"data-panel-auto-save="true""#), "{html}");
        assert!(html.contains("Held values will come back after a restart."));

        let html = render_section(Some(scope), 0, None);
        assert!(html.contains("None held"), "{html}");
        assert!(!html.contains("data-panel-reset"), "nothing to let go of");
        assert!(
            !html.contains("data-panel-auto-save"),
            "below the edit tier"
        );
    }

    #[test]
    fn the_words_say_the_state() {
        assert_eq!(held_words(0), "None held");
        assert_eq!(held_words(1), "1 held");
        assert_eq!(held_words(4), "4 held");
        assert_eq!(
            auto_save_words(false),
            "Held values will be lost on a restart."
        );
    }

    /// No box in a box: a section of the details card.
    #[test]
    fn the_settings_are_a_section_with_no_frame() {
        assert!(SECTION_CLASS.contains("tw:border-t"));
        for class in [SECTION_CLASS, ROW_CLASS] {
            assert!(!class.contains("rounded"), "{class}");
            assert!(!class.contains("tw:bg-"), "{class}");
        }
    }

    fn render_section(
        target: Option<lpc_wire::WireScopeRef>,
        held: usize,
        auto_save: Option<bool>,
    ) -> String {
        render(
            Root,
            RootProps {
                target,
                held,
                auto_save,
            },
        )
    }

    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn Root(
        target: Option<lpc_wire::WireScopeRef>,
        held: usize,
        auto_save: Option<bool>,
    ) -> Element {
        rsx! {
            PanelSettingsSection { target, held, auto_save, on_action: |_| {} }
        }
    }
}
