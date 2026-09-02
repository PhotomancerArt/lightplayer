use dioxus::prelude::*;
use lpa_studio_core::{ControllerId, ProjectEditorOp, UiAction};
use lpa_studio_web_story_macros::story;

use crate::app::module::module_fixtures::{
    fire_export, inline_module_export, module_card_with_export,
};
use crate::app::node::node_story_fixtures::{
    debug_rows_node_view, error_node_view, failed_dirty_node_view, fault_node_view,
    nested_dirty_node_view, node_delete_pane_action, output_node_view, playlist_node_view,
    playlist_pending_edits, unsaved_dirty_node_view, unsupported_node_view,
};
use crate::app::node::{NodeDetailPopover, NodeDirtyTint, NodePane};

/// Story stand-in for the controller-built focus action so panes render the
/// header select control.
fn story_focus_action() -> UiAction {
    UiAction::from_op(ControllerId::new("story.module"), ProjectEditorOp::Focus)
}

#[story(description = "A composed node pane showing the current node anatomy direction.")]
pub(crate) fn node_pane() -> Element {
    let mut view = playlist_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(
    description = "A selected node pane collapsed down to its header: selection border and active select control."
)]
pub(crate) fn collapsed_node_pane() -> Element {
    let mut view = playlist_node_view();
    view.action = Some(story_focus_action());
    view.focused = true;
    view.collapsed = true;

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(description = "Node pane with an error status and projection issues.")]
pub(crate) fn error_node() -> Element {
    rsx! {
        NodePane { view: error_node_view() }
    }
}

#[story(
    description = "Node pane header with the always-available delete action (Trash2, generic pane-action path): pressing it runs the confirmation composed core-side from the removal pre-flight — dependents count, pending edits the sweep discards, and the files staged for deletion on save."
)]
pub(crate) fn header_delete_action() -> Element {
    let mut view = playlist_node_view();
    view.action = Some(story_focus_action());
    view.header_actions = vec![node_delete_pane_action()];

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(
    description = "D7 variant (a), unsaved: header-only yellow tint; the yellow edit-pencil detail trigger is the whole announcement (no count chips — counts live in the popup)."
)]
pub(crate) fn dirty_unsaved_header_tint() -> Element {
    let mut view = unsaved_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane {
            view,
            on_action: move |_| {},
            dirty_tint: NodeDirtyTint::HeaderOnly,
        }
    }
}

#[story(
    description = "D7 variant (b), unsaved: the yellow tint re-mixed into the whole pane surface."
)]
pub(crate) fn dirty_unsaved_surface_tint() -> Element {
    let mut view = unsaved_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane {
            view,
            on_action: move |_| {},
            dirty_tint: NodeDirtyTint::FullSurface,
        }
    }
}

#[story(
    description = "D7 variant (a), failed: the error wash dominates the header and the detail trigger wears the red warning glyph (the live default)."
)]
pub(crate) fn dirty_failed_header_tint() -> Element {
    let mut view = failed_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane {
            view,
            on_action: move |_| {},
            dirty_tint: NodeDirtyTint::HeaderOnly,
        }
    }
}

#[story(
    description = "D7 variant (b), failed: the error tint re-mixed into the whole pane surface."
)]
pub(crate) fn dirty_failed_surface_tint() -> Element {
    let mut view = failed_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane {
            view,
            on_action: move |_| {},
            dirty_tint: NodeDirtyTint::FullSurface,
        }
    }
}

#[story(
    description = "Dirty bubbling: a dirty grandchild's affordance shows on its own detail trigger and on both ancestors' triggers (with the header tint), so a collapsed parent still reveals a dirty descendant; the clean sibling stays silent."
)]
pub(crate) fn nested_dirty_children() -> Element {
    let mut view = nested_dirty_node_view();
    view.action = Some(story_focus_action());

    rsx! {
        NodePane { view, on_action: move |_| {} }
    }
}

#[story(
    description = "The live default: a card with three Debug-role fields (a story-only probe specimen — the clock's transport rows retired into its tape face, and the real in-tree Debug slot, the output's `test_pattern`, has only one row). The section is COLLAPSED — most of the time those controls are not wanted — but its header is always debug territory: hazard-striped, labelled DEBUG, reading \"session only\". Nothing is overridden, so there is no count, no Clear, and no card marking. The persisted rows sit above under `Settings`."
)]
pub(crate) fn debug_section_idle() -> Element {
    rsx! {
        NodePane { view: debug_rows_node_view(0, false), on_action: move |_| {} }
    }
}

#[story(
    description = "Collapsed with two active overrides: the header reads \"2 active · session only\" and offers Clear WITHOUT expanding, and the card header carries the `debug 2` marking. The header box is the same height as the idle story's — the count and the Clear button are reserved space, so touching a control never reflows the card."
)]
pub(crate) fn debug_section_collapsed_active() -> Element {
    rsx! {
        NodePane { view: debug_rows_node_view(2, false), on_action: move |_| {} }
    }
}

#[story(
    description = "Expanded with two active overrides: the flattened Debug rows (Enabled / Gain / Window seconds — no nested record group), the touched ones wearing the hazard row tint with the inline Clear verb. The header wash stays neutral on purpose — a debug override is NOT unsaved work (D7), so it never borrows the amber dirty treatment."
)]
pub(crate) fn debug_section_active() -> Element {
    rsx! {
        NodePane { view: debug_rows_node_view(2, true), on_action: move |_| {} }
    }
}

#[story(
    description = "Expanded and idle: what the disclosure reveals before anything is touched — three transient controls, each already reading as debug territory. This is the clean-transient case D8c exists for."
)]
pub(crate) fn debug_section_expanded_idle() -> Element {
    rsx! {
        NodePane { view: debug_rows_node_view(0, true), on_action: move |_| {} }
    }
}

#[story(
    description = "The hazard family beside its neighbours, for the G1 distinctness question: collapsed-idle, collapsed-active, and expanded-active Debug sections next to an amber-unsaved card. Debug = attention-orange + diagonal stripes; flat orange stays device health; amber stays unsaved. The three debug cards also show the no-reflow contract — every header strip is the same height."
)]
pub(crate) fn debug_section_vs_unsaved() -> Element {
    let mut unsaved = unsaved_dirty_node_view();
    unsaved.action = Some(story_focus_action());

    rsx! {
        div { class: "tw:grid tw:gap-4",
            NodePane { view: debug_rows_node_view(0, false), on_action: move |_| {} }
            NodePane { view: debug_rows_node_view(2, false), on_action: move |_| {} }
            NodePane { view: debug_rows_node_view(2, true), on_action: move |_| {} }
            NodePane { view: unsaved, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "The P5 proof case, hardware mode: an Output card whose one Debug field is `test_pattern`. Expanded with the override ACTIVE — the strip on `ws281x:local:D10` is solid white and the engine skips the graph resolve entirely for this output. The card wears the `debug 1` marking, the striped header offers Clear, and the row carries the hazard tint; endpoint and driver options stay above under `Settings`. Nothing here is output-specific UI: the section is derived from `SlotRole::Debug` (P1) by the same generic partition — and since the clock's transport rows retired into its tape face, this is the one real in-tree Debug row."
)]
pub(crate) fn output_debug_test_pattern_active() -> Element {
    rsx! {
        NodePane { view: output_node_view(true, true), on_action: move |_| {} }
    }
}

#[story(
    description = "The same Output card at rest, collapsed: one Debug field, nothing overridden, so no count, no Clear, no card marking — but the header still reads as debug territory. The live default for a hardware output nobody is probing."
)]
pub(crate) fn output_debug_test_pattern_idle() -> Element {
    rsx! {
        NodePane { view: output_node_view(false, false), on_action: move |_| {} }
    }
}

#[story(
    description = "The node detail popup on an erroring node: the status pill plus the runtime's error text — the popup answers WHY a node is in the error state (the compact status alone doesn't)."
)]
pub(crate) fn error_detail_popup() -> Element {
    let view = error_node_view();

    rsx! {
        div { class: "tw:flex tw:min-h-[320px] tw:justify-end",
            NodeDetailPopover {
                header: view.header,
                pending_edits: vec![],
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

#[story(
    description = "The node detail popup on a FAULTED node: the same error-red pill as a compile error, reading 'Fault', with the runtime's own reason underneath — here a compile the crash-recovery ledger refused after repeated crashes. The word is the affordance: an Error asks for an edit, a Fault asks for a retry or a clear, and the source in this node is fine."
)]
pub(crate) fn fault_detail_popup() -> Element {
    let view = fault_node_view();

    rsx! {
        div { class: "tw:flex tw:min-h-[320px] tw:justify-end",
            NodeDetailPopover {
                header: view.header,
                pending_edits: vec![],
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

#[story(
    description = "A node whose kind this device's firmware does not carry. The pane body is GONE — the fixture has two config slots and a second tab, and none of it renders: there is no runtime here, so there is no live state to show and no edit that could take effect. The whole body region below the header becomes the message instead: hazard-striped error red, edge to edge (no box inside the box), one plain sentence naming the kind, and a link to the boards catalog. G1 round 3 — dimmed/neutral (too quiet) and warning-yellow-in-a-bordered-block (crowded, box-in-a-box) were both rejected before this."
)]
pub(crate) fn unsupported_node() -> Element {
    rsx! {
        NodePane { view: unsupported_node_view() }
    }
}

#[story(
    description = "The detail popup on a not-on-this-device node: the error-family status pill reading 'Not on this device' plus the engine's own reason ('node kind Fluid is not included in this firmware build') — the build-level wording lives HERE, keeping the pane's own message to one clean sentence."
)]
pub(crate) fn unsupported_detail_popup() -> Element {
    let view = unsupported_node_view();

    rsx! {
        div { class: "tw:flex tw:min-h-[320px] tw:justify-end",
            NodeDetailPopover {
                header: view.header,
                pending_edits: vec![],
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

#[story(
    description = "The merged node detail popup open: status content plus the per-bucket dirty sections as tinted-title change lists — the node's OWN pending edits with per-entry reverts (subtree counts ride the title rows; the other node's edit in the threaded list is filtered out)."
)]
pub(crate) fn dirty_detail_popup() -> Element {
    let view = unsaved_dirty_node_view();

    rsx! {
        div { class: "tw:flex tw:min-h-[620px] tw:justify-end",
            NodeDetailPopover {
                header: view.header,
                pending_edits: playlist_pending_edits(),
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

/// The module popup as the export stories render it: the card's own face
/// carries the designation row, exactly as `NodePane` threads it in the app.
fn export_popup(name: &str, export: lpa_studio_core::UiModuleExport) -> Element {
    let view = module_card_with_export(name, export);
    let Some(lpa_studio_core::UiNodeFace::Module(face)) = view.face.clone() else {
        unreachable!("the fixture card wears a module face")
    };

    rsx! {
        div { class: "tw:flex tw:min-h-[760px] tw:items-start tw:justify-end",
            NodeDetailPopover {
                header: view.header,
                pending_edits: vec![],
                module: face,
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

#[story(
    description = "The module detail popup's EXPORT section (module authoring unit, P3), on a module that is not exported yet: the sage-titled section, a checkbox naming the PROJECT (the designation is manifest data even though the gesture lives on the module you are looking at), the hint saying what an importer actually gets, and the upgrade sentence — the first export is what makes a general project a pattern project (vision D14). Provenance sits in the identity rows above, because provenance is what an importer inherits."
)]
pub(crate) fn module_export_popup_offered() -> Element {
    export_popup("fire", fire_export(false))
}

#[story(
    description = "The same section once the module IS exported: the box is ticked, the upgrade sentence is gone (the project is already a pattern project), and this export's lint findings render in place — a warning (its channel's only writer is scaffolding, so imported copies run on the authored default) and a hard error (a file reference escapes the folder). The two severities keep their own tones inside a sage section: the family colour says what kind of thing this is, the finding tone says how it is doing."
)]
pub(crate) fn module_export_popup_lint() -> Element {
    export_popup("fire", fire_export(true))
}

#[story(
    description = "The DISABLED row: a module that is a single file has no folder to vendor, so the checkbox is inert and the row explains itself rather than disappearing (the add-node picker's disabled-row precedent). The same shape carries the other refusals — a module nested deeper than one level, and a device session, where the manifest you would be editing is the library's, not the one in front of you."
)]
pub(crate) fn module_export_popup_disabled() -> Element {
    export_popup("wave", inline_module_export())
}
