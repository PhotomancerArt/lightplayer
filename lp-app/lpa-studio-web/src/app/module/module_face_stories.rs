//! Stories for the module face (`docs/design/modules.md` §5).
//!
//! These carry G2 questions 1–3: does one face hold up at every zoom
//! level, is the controls/wiring split right, and does the root module back
//! in the node area read better or worse?
//!
//! The card chrome is the real [`NodePane`] — header, kind label, collapse
//! — and the children below it are real sibling cards on the real
//! [`crate::app::node::NodeChildren`] rail, so what is being judged is the
//! face inside a genuine workspace column, not a mock frame.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;

use super::module_fixtures::{
    HELD, PLASMA_1_SCOPE, PanelWalk, ROOT_SCOPE, clean_exports_view, control_root_face,
    designated_export, error_exports_view, fire_export, held_root_face, held_root_view,
    module_card_with_export, module_node_view, plasma_children, plasma_face, plasma_one_panel,
    root_module_node_view, scaffolding_warning, warning_exports_view,
};
use super::{ModuleFace, PanelGesture};

/// The workspace column's width, so a card story shows the same measure the
/// app does.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn WorkspaceCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-[720px]", {children} }
    }
}

#[story(
    description = "The root module as the single top-level workspace card (flat-root reversal): output-mirror hero, the scope's panel with two nested effect groups, wiring drawer, provenance. Its children expand BELOW it as full sibling cards on the usual rail — all of them, since a module's children are collaborators, not branches. Walkable: turn a knob and it engages (amber), reset and it follows the project again."
)]
fn root_card() -> Element {
    // The walkable fixture: the card view is DERIVED from it each render,
    // so a knob drag really does move the knob and engage the control —
    // on the panel and on the child card that shares its identity.
    let mut walk = use_signal(|| PanelWalk::new(root_module_node_view()).with_held(HELD));

    rsx! {
        WorkspaceCanvas {
            NodePane {
                view: walk().view.clone(),
                module_panel: move |gesture: PanelGesture| {
                    walk.with_mut(|walk| walk.apply_gesture(&gesture));
                },
                on_action: move |action| {
                    walk.with_mut(|walk| walk.apply_action(&action));
                },
            }
        }
    }
}

#[story(
    description = "The presentation overlap the model implies, cropped to the two cards that show it: plasma_1's speed appears as a nested group on the host's panel AND on plasma_1's own card below. Both are amber, because they are the same (scope, channel) control — one control, two views (panel.md P1), the same way a playlist's bound control shows on the parent face and the child card. Kept deliberately, not suppressed."
)]
fn author_zoom() -> Element {
    let mut view = held_root_view();
    // Only the embedded module, so the comparison is the whole picture.
    view.children
        .retain(|child| child.label.starts_with("plasma_1"));
    rsx! {
        WorkspaceCanvas {
            NodePane { view, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "Artist zoom: one embedded effect module as a card on its own — the identical face, one level in. Output mirror, its scope's panel, wiring drawer, provenance line, and its own child shader on the same nesting rail below it. The rail is the grammar at every depth; the face never changes."
)]
fn embedded_module() -> Element {
    let view = module_node_view(
        "plasma_1",
        PLASMA_1_SCOPE,
        "effect · detached",
        plasma_face(plasma_one_panel(), 3.1),
    )
    .with_children(plasma_children(3.1));
    rsx! {
        div { class: "tw:w-full tw:max-w-md",
            NodePane { view, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "Bus split, drawer half: the wiring drawer open on the root module — every channel in this scope with its writers and readers, i.e. exactly the retired sidebar bus pane's content, now hung off the module that owns the scope. The panel above is the same bus presented for playing. The rows themselves are unchanged (see studio/module/wiring-drawer/*)."
)]
fn wiring_drawer_open() -> Element {
    let mut face = held_root_face();
    face.wiring_open = true;
    rsx! {
        WorkspaceCanvas {
            NodePane {
                view: module_node_view("Aurora Sign", ROOT_SCOPE, "5 nodes · 2 effects", face),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Nested panel groups (R8) as bordered clusters SIDE BY SIDE: two instances of one effect, each a light hairline box with its name in the top border, laid out in a wrapping flex row — function groups on hardware. plasma 1 is detached (amber label, amber reset in its border), plasma 2 still follows the host. Same channel names, different scopes, independent controls; the label carries the instance identity now that the path lives in its popup. No collapse — wrapping is the density mechanism."
)]
fn nested_groups() -> Element {
    let mut face = held_root_face();
    face.preview = None;
    face.wiring = None;
    rsx! {
        WorkspaceCanvas {
            div { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                ModuleFace { face, on_action: move |_| {} }
            }
        }
    }
}

#[story(
    label = "Control Output",
    description = "A control-first module: no channel in the scope carries a visual, so the mirror would render cleared — the hero is the scope's `control.out` product instead, the fixture's lamp layout drawn by the same preview component the fixture card uses. The wiring drawer below shows where it comes from: the fixture writes it, the hardware output reads it."
)]
fn control_output() -> Element {
    let mut face = control_root_face();
    face.wiring_open = true;
    rsx! {
        WorkspaceCanvas {
            NodePane {
                view: module_node_view("Scanner Rig", ROOT_SCOPE, "3 nodes · 1 fixture", face),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The workspace column with the sidebar bus pane GONE — deleted in P3, not hidden: the root module card heads the column and the bus lives on it, controls on the face and wiring in the drawer, with the project's nodes below it as the sibling cards they always were."
)]
fn workspace_no_bus_pane() -> Element {
    let mut view = held_root_view();
    if let Some(lpa_studio_core::UiNodeFace::Module(face)) = view.face.as_mut() {
        face.wiring_open = true;
    }
    rsx! {
        div { class: "tw:grid tw:w-full tw:max-w-[860px] tw:gap-3.5 tw:bg-page tw:p-3",
            NodePane { view, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "A PATTERN project's workspace column, grouped (module authoring unit, G1 R-A): the root card, then its children split into what this project HANDS OUT and what stays home. The `exports` header wears the sage export family and the effect folders' cards sit under it, each with the display-only chip on its header; `rig` collects the clock, the shared non-exported `common` module, and the fixture — D17's word for scaffolding-as-context. Both exports read clean, so there is no preamble at all. P3 put this on the root card as a rail of names; here the exports are the cards themselves, so 'what does this ship' is answered by where a card sits."
)]
fn child_grouping_clean() -> Element {
    rsx! {
        WorkspaceCanvas {
            NodePane { view: clean_exports_view(), on_action: move |_| {} }
        }
    }
}

#[story(
    description = "The same column with one export carrying a WARNING: fire reads a channel only the non-exported `common` writes, so an imported copy runs on the authored default. The aggregate line sits directly under the `exports` header, above the cards — say it once for the group — and fire's own chip takes the warning tone so the sentence and the card that owns it are connected without a name lookup. The `rig` half stays untouched: nothing about a warning changes what scaffolding is."
)]
fn child_grouping_warning() -> Element {
    rsx! {
        WorkspaceCanvas {
            NodePane { view: warning_exports_view(), on_action: move |_| {} }
        }
    }
}

#[story(
    description = "Both severities at once: a warning and a hard ERROR (a file inside ripple_interference_cascade points outside the folder, so the vendored copy would not load). Two preamble lines rank worst-last in report order, and each card's chip carries its OWN verdict — warning on fire, error on ripple, plain sage on the clean one. This is the story that says whether the group can carry bad news: the error must not be swallowed by the warning above it, and the sage header must not soften either."
)]
fn child_grouping_error() -> Element {
    rsx! {
        WorkspaceCanvas {
            NodePane { view: error_exports_view(), on_action: move |_| {} }
        }
    }
}

#[story(
    description = "The chip alone, four ways, on cards out of any column: clean (sage), warning, error, and undesignated. The chip is DISPLAY only (D12 — it never toggles anything; the gesture lives in the popup behind the ⓘ), and its tone is this export's own lint verdict, so a card that would ship badly says so where you can see it. Read top to bottom: the chip has to be findable without being loud, and the three tones have to be distinguishable at chip size."
)]
fn child_card_export_chip() -> Element {
    rsx! {
        WorkspaceCanvas {
            div { class: "tw:grid tw:gap-3",
                NodePane {
                    view: module_card_with_export("noise_party", designated_export("noise_party", Vec::new())),
                    on_action: move |_| {},
                }
                NodePane {
                    view: module_card_with_export(
                        "fire",
                        designated_export("fire", vec![scaffolding_warning()]),
                    ),
                    on_action: move |_| {},
                }
                NodePane {
                    view: module_card_with_export("ripple_interference_cascade", fire_export(true)),
                    on_action: move |_| {},
                }
                NodePane {
                    view: module_card_with_export("common", fire_export(false)),
                    on_action: move |_| {},
                }
            }
        }
    }
}
