//! Stories for the module face (M2 UX spike, gate G2).
//!
//! These carry G2 questions 1–3: does one face hold up at every zoom
//! level, is the controls/wiring split right, and does the root module back
//! in the node area read better or worse?
//!
//! The card chrome is the real [`NodePane`] — header, kind label, collapse
//! — so what is being judged is the face inside a genuine workspace card,
//! not a mock frame.

use dioxus::prelude::*;
use lpa_studio_core::UiNodeFace;
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;

use super::module_fixtures::{
    HELD, PLASMA_1_SCOPE, PanelSpike, ROOT_SCOPE, held_root_face, module_node_view, plasma_face,
    plasma_one_panel, root_face, root_module_node_view,
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
    description = "The root module as the single top-level workspace card (flat-root reversal): output-mirror hero, the scope's panel with two nested effect groups, children nested INSIDE the card, and the wiring drawer. Walkable — turn a knob and it engages (amber), reset and it follows the project again."
)]
fn root_card() -> Element {
    // The walkable fixture: the card view is DERIVED from it each render,
    // so a knob drag really does move the knob and engage the control.
    let mut spike = use_signal(|| PanelSpike::new(root_face()).with_held(HELD));
    let mut view = root_module_node_view();
    view.face = Some(UiNodeFace::Module(spike().face.clone()));

    rsx! {
        WorkspaceCanvas {
            NodePane {
                view,
                module_panel: move |gesture: PanelGesture| {
                    spike.with_mut(|spike| spike.apply_gesture(&gesture));
                },
                on_action: move |action| {
                    spike.with_mut(|spike| spike.apply_action(&action));
                },
            }
        }
    }
}

#[story(
    description = "Author zoom: every child expanded inside the root card, including both embedded plasma modules wearing the SAME face one level in. Shows the presentation overlap the model implies — an embedded module's controls appear both as a nested panel group above and on its own child card below."
)]
fn author_zoom() -> Element {
    let mut face = held_root_face();
    for child in &mut face.children {
        child.collapsed = false;
    }
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
    description = "Artist zoom: one embedded effect module as a card on its own — the identical face, one level in. Output mirror, its scope's panel, its child shader, wiring drawer, provenance line."
)]
fn embedded_module() -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md",
            NodePane {
                view: module_node_view(
                    "plasma_1",
                    PLASMA_1_SCOPE,
                    "effect · detached",
                    plasma_face(plasma_one_panel(), 3.1),
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Bus split, drawer half: the wiring drawer open on the root module — every channel in this scope with its writers and readers, i.e. exactly today's sidebar bus-pane content, now hung off the module that owns the scope. The panel above is the same bus presented for playing."
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
    description = "Nested panel groups (R8): two instances of one effect side by side on the root panel — plasma_1 expanded and detached (amber), plasma_2 collapsed to its summary row and still following the host. Same channel names, different scopes, independent controls."
)]
fn nested_groups() -> Element {
    let mut face = held_root_face();
    face.preview = None;
    face.children.clear();
    face.wiring = None;
    if let Some(second) = face.panel.groups.get_mut(1) {
        second.collapsed = true;
    }
    rsx! {
        WorkspaceCanvas {
            div { class: "tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card",
                ModuleFace { face, on_action: move |_| {} }
            }
        }
    }
}

#[story(
    description = "The workspace column with the sidebar bus pane GONE: the root module card is the whole node area, and the bus lives on it — controls on the face, wiring in the drawer. Compare against studio/bus/bus-pane/fyeah-sign, which is what this replaces."
)]
fn workspace_no_bus_pane() -> Element {
    let mut face = held_root_face();
    face.wiring_open = true;
    if let Some(second) = face.panel.groups.get_mut(1) {
        second.collapsed = true;
    }
    rsx! {
        div { class: "tw:grid tw:w-full tw:max-w-[860px] tw:gap-3.5 tw:bg-page tw:p-3",
            NodePane {
                view: module_node_view("Aurora Sign", ROOT_SCOPE, "5 nodes · 2 effects", face),
                on_action: move |_| {},
            }
        }
    }
}
