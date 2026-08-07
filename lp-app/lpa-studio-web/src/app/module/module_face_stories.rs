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
    EXPORT_PROJECT, HELD, PLASMA_1_SCOPE, PanelWalk, ROOT_SCOPE, clean_exports, control_root_face,
    error_exports, exporting_root_face, fire_export, held_root_face, held_root_view,
    module_card_with_export, module_node_view, plasma_children, plasma_face, plasma_one_panel,
    root_module_node_view, warning_exports,
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
    description = "The root card of a PATTERN project: the exports section (module authoring unit, P3) sits between the wiring drawer and the provenance footer, wearing the sage export wash and a sage rail. Three folders, every one clean — sage dots, no lint lines. Sage is the whole family's hue: it reads as a structural property of the project ('this ships from here'), not as a status you would clear. A project that exports nothing has no section at all, which is what keeps a plain project plain (spike 2·ii)."
)]
fn root_card_exports_clean() -> Element {
    rsx! {
        WorkspaceCanvas {
            NodePane {
                view: module_node_view(
                    EXPORT_PROJECT,
                    ROOT_SCOPE,
                    "5 nodes · 3 exports",
                    exporting_root_face(clean_exports()),
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The same section with one export carrying a WARNING: fire reads a channel only scaffolding writes, so an imported copy runs on the authored default. The row's dot takes the warning tone while the section keeps its sage identity — the family colour says what kind of thing this is, the dot says how it is doing. The aggregate line under the rows is the same sentence the module's own popup shows."
)]
fn root_card_exports_warning() -> Element {
    rsx! {
        WorkspaceCanvas {
            NodePane {
                view: module_node_view(
                    EXPORT_PROJECT,
                    ROOT_SCOPE,
                    "5 nodes · 2 exports",
                    exporting_root_face(warning_exports()),
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Both severities at once: a warning and a hard ERROR (a file inside the export folder points outside it, so the vendored copy would not load). The error dot must not be swallowed by the warning above it, and the two lint lines rank worst-last in report order — this is the story that says whether the section can carry bad news without the sage wash softening it."
)]
fn root_card_exports_error() -> Element {
    rsx! {
        WorkspaceCanvas {
            NodePane {
                view: module_node_view(
                    EXPORT_PROJECT,
                    ROOT_SCOPE,
                    "5 nodes · 3 exports",
                    exporting_root_face(error_exports()),
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "A designated child module card: the header wears the display-only `export` chip (D12 — the chip never toggles anything; the gesture lives in the popup behind the ⓘ). Beside it, the same card undesignated, so the two read against each other — the chip is the only difference, and it has to be findable without being loud."
)]
fn child_card_export_chip() -> Element {
    rsx! {
        WorkspaceCanvas {
            div { class: "tw:grid tw:gap-3",
                NodePane {
                    view: module_card_with_export("fire", fire_export(true)),
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
