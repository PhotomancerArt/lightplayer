//! Stories for the module face (M2 UX spike, gate G2).
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
    HELD, PLASMA_1_SCOPE, PanelSpike, ROOT_SCOPE, held_root_face, held_root_view, module_node_view,
    plasma_children, plasma_face, plasma_one_panel, root_module_node_view,
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
    let mut spike = use_signal(|| PanelSpike::new(root_module_node_view()).with_held(HELD));

    rsx! {
        WorkspaceCanvas {
            NodePane {
                view: spike().view.clone(),
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
    description = "Bus split, drawer half: the wiring drawer open on the root module — every channel in this scope with its writers and readers, i.e. exactly today's sidebar bus-pane content, now hung off the module that owns the scope. The panel above is the same bus presented for playing. Left as-is this round; the bus/wiring view gets its own UX spike."
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
    description = "The workspace column with the sidebar bus pane GONE: the root module card heads the column and the bus lives on it — controls on the face, wiring in the drawer — with the project's nodes below it as the sibling cards they always were. Compare against studio/bus/bus-pane/fyeah-sign, which is what this replaces."
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
