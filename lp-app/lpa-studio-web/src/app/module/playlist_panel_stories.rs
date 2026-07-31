//! Stories for playlist meta-switching (M2 UX spike; `modules.md` E2).
//!
//! A playlist is an *isolating* node: each entry gets its own anonymous
//! sink scope (R2), so the same channel name lists separately per entry and
//! neither entry surfaces on the host's panel. The playlist's face presents
//! the **active** entry's panel, and switching entries re-derives the
//! control from whatever is bound in the newly active sink scope (R9):
//! 0–1 "Drift" becomes 0–10 "Whirl" — same channel, different control.
//!
//! Panel state is per `(scope, channel)` (P1/R10), so tweaking Drift,
//! switching to Whirl, and switching back finds Drift's tweak still there.
//! That is what the walkable story demonstrates: Drift starts held at 0.35
//! while Whirl is untouched at its authored default.

use dioxus::prelude::*;
use lpa_studio_core::UiPlaylistFace as UiPlaylistFaceData;
use lpa_studio_web_story_macros::story;

use crate::app::node::{NodeCardSection, PlaylistFace};

use super::module_fixtures::{
    PanelSpike, entry_held_panel, entry_panel, entry_scope, playlist_face,
};
use super::{ModulePanel, PanelGesture};

/// The playlist card: the entries strip (its own kind-specific face) above
/// the ACTIVE entry's panel. The strip is the face; the panel below it is
/// the sink child's, hosted here because inactive siblings surface nowhere.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PlaylistCard(
    entries: UiPlaylistFaceData,
    entry: u32,
    #[props(default = None)] on_select: Option<EventHandler<u32>>,
    children: Element,
) -> Element {
    let scope = entry_scope(entry);
    rsx! {
        div { class: "tw:w-full tw:max-w-md tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
            PlaylistFace { face: entries.clone(), on_action: move |_| {} }
            // Entry switching has no wire op today (activation-by-click is
            // still an open authoring gesture), so the spike exposes it as
            // plain buttons — the point being measured is what happens to
            // the PANEL, not how the entry gets activated.
            if let Some(on_select) = on_select {
                div { class: "tw:flex tw:flex-wrap tw:gap-2 tw:border-t tw:border-border-strong tw:px-4 tw:py-2",
                    for candidate in entries.entries.clone() {
                        button {
                            key: "{candidate.key}",
                            class: if candidate.key == entry {
                                "tw:cursor-pointer tw:rounded-xs tw:border tw:border-status-live-border tw:bg-status-live-bg tw:px-2 tw:py-0.5 tw:text-[11px] tw:text-status-live-foreground"
                            } else {
                                "tw:cursor-pointer tw:rounded-xs tw:border tw:border-border tw:bg-transparent tw:px-2 tw:py-0.5 tw:text-[11px] tw:text-muted-foreground"
                            },
                            r#type: "button",
                            onclick: move |_| on_select.call(candidate.key),
                            "activate {candidate.name}"
                        }
                    }
                }
            }
            NodeCardSection { label: "panel",
                div { class: "tw:px-4 tw:pt-2 tw:text-[0.6rem] tw:text-dim-foreground",
                    "active entry's sink scope "
                    code { class: "tw:font-mono", "{scope}" }
                }
                {children}
            }
        }
    }
}

#[story(
    description = "Entry A active: the speed channel binds a 0–1 slot labelled Drift, and it is held (amber) at 0.35. This is the control the panel derives from the ACTIVE sink scope."
)]
fn entry_drift() -> Element {
    rsx! {
        PlaylistCard { entries: playlist_face(), entry: 0,
            ModulePanel {
                panel: entry_held_panel(0),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Entry B active: the SAME channel name now binds a 0–10 slot labelled Whirl, so the control re-derives — new label, new range, new resting position — and it is at its own authored default, untouched by anything done to Drift."
)]
fn entry_whirl() -> Element {
    let mut entries = playlist_face();
    entries.active = Some(1);
    rsx! {
        PlaylistCard { entries, entry: 1,
            ModulePanel {
                panel: entry_panel(1),
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Walkable meta-switch: activate Drift or Whirl and watch the control swap under you — 0–1 held-amber vs 0–10 at-default. Tweak one, switch away, switch back: per-(scope, channel) state means the tweak is still there, because the two entries are two different sink scopes."
)]
fn meta_switch() -> Element {
    let mut entry = use_signal(|| 0_u32);
    // Drift starts with a tweak already on it — the thing that has to
    // still be there after a round trip through Whirl.
    let mut drift = use_signal(|| {
        let scope = entry_scope(0);
        PanelSpike::new(spike_face(0)).with_held(&[(scope.as_str(), "speed", 0.35)])
    });
    let mut whirl = use_signal(|| PanelSpike::new(spike_face(1)));
    let mut entries = playlist_face();
    entries.active = Some(entry());
    let active = entry();
    let panel = if active == 0 {
        drift().face.panel.clone()
    } else {
        whirl().face.panel.clone()
    };

    rsx! {
        PlaylistCard {
            entries,
            entry: active,
            on_select: move |key| entry.set(key),
            ModulePanel {
                panel,
                on_panel: move |gesture: PanelGesture| {
                    if active == 0 {
                        drift.with_mut(|spike| spike.apply_gesture(&gesture));
                    } else {
                        whirl.with_mut(|spike| spike.apply_gesture(&gesture));
                    }
                },
                on_action: move |action| {
                    if active == 0 {
                        drift.with_mut(|spike| spike.apply_action(&action));
                    } else {
                        whirl.with_mut(|spike| spike.apply_action(&action));
                    }
                },
            }
        }
    }
}

/// One entry's panel wrapped as walkable spike state. Each entry gets its
/// OWN state holder — that separation is the model's, not a story
/// convenience: two sink scopes are two identities (P1).
fn spike_face(entry: u32) -> lpa_studio_core::UiModuleFace {
    lpa_studio_core::UiModuleFace::new(entry_panel(entry))
}
