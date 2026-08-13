//! The unified editor's center — the coordinator mounted in the
//! workbench Mapping view (unified-editor P3, workbench-amended).
//!
//! The workbench chrome (#413) owns the docks, panels, and view tabs;
//! this module owns only the CENTER: the editor toolbar strip and the
//! Arrange canvas pane (P4 mounts the canvas; this phase is the mount
//! point plus the flag-driven prefetch). The Fixtures/Outputs panels are
//! the editor's rails — they are grown in place, never forked.
//!
//! Mode note (Yona, 2026-08-12 mid-run steer): mapping lands FIRST and
//! patching's home is decided after it is played with — so there is no
//! mapping|patching mode segment here yet, and the interim `/patch` page
//! stays untouched. The toolbar keeps the slot the segment (or whatever
//! wins) will occupy.

use dioxus::prelude::*;
use lpa_studio_core::{ProjectController, UiAction, UiPatchSurface, UiPatchTarget};

/// The Mapping view's center: toolbar + canvas pane.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn EditorShellCenter(
    surface: Option<UiPatchSurface>,
    selection: Option<UiPatchTarget>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let Some(surface) = surface else {
        return rsx! {
            div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:items-center tw:justify-center",
                p { class: "tw:m-0 tw:max-w-[360px] tw:text-center tw:text-xs tw:text-dim-foreground",
                    "No fixtures on a wire yet — bind an output to a control bus and the mapping editor fills in."
                }
            }
        };
    };
    prefetch_editor_meta(&on_action, &surface);
    let fixtures = surface.fixtures.len();
    let arranged = surface
        .fixtures
        .iter()
        .filter(|fixture| {
            fixture
                .arrange
                .as_ref()
                .is_some_and(|arrange| arrange.arranged)
        })
        .count();
    rsx! {
        div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:flex-col",
            // The editor toolbar: mapping tools mount here (P5); the right
            // end is the reserved slot for whatever patching's home turns
            // out to be. Slim, like the dock tab rows.
            div { class: "tw:flex tw:min-h-[30px] tw:flex-none tw:items-center tw:gap-2 tw:border-b tw:border-border-subtle tw:bg-card-muted tw:px-2.5",
                span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-muted-foreground",
                    "Arrange"
                }
                span { class: "tw:ml-auto tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                    "{fixtures} fixtures · {arranged} arranged"
                }
            }
            if let Some(error) = surface.editor_meta_error.clone() {
                div { class: "tw:flex-none tw:border-b tw:border-border-subtle tw:bg-status-attention-bg tw:px-2.5 tw:py-1 tw:text-[11px] tw:text-status-attention-foreground",
                    "editor.json refused: {error} — arranging is disabled so the file is never rewritten blind."
                }
            }
            // The canvas pane (P4 mounts the arrange canvas here).
            div { class: "tw:flex tw:min-h-0 tw:flex-1 tw:items-center tw:justify-center",
                if !surface.editor_meta_loaded {
                    p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "Loading the arrangement…" }
                } else {
                    div { class: "tw:rounded-lg tw:border tw:border-dashed tw:border-border-strong tw:px-8 tw:py-6 tw:text-center",
                        p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-muted-foreground",
                            "Arrange canvas"
                        }
                        p { class: "tw:m-0 tw:mt-1 tw:text-xs tw:text-dim-foreground",
                            "Fixture geometry lands here next — select fixtures in the panels meanwhile."
                        }
                    }
                }
            }
        }
    }
}

/// Flag-driven prefetch (the #409 lesson: never hand-code a fetch a flag
/// doesn't ask for): while the surface says editor.json has not settled,
/// dispatch the fetch. Absence settles the flag too, so this quiesces
/// after one round trip.
fn prefetch_editor_meta(on_action: &EventHandler<UiAction>, surface: &UiPatchSurface) {
    if !surface.editor_meta_loaded
        && let Some(artifact) = surface.editor_meta_artifact.clone()
    {
        on_action.call(UiAction::from_op(
            ProjectController::NODE_ID,
            lpa_studio_core::EditorMetaFetchOp { artifact },
        ));
    }
}
