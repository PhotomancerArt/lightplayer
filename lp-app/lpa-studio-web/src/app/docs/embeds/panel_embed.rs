//! `panel`: the lensed project's root panel, rendered by the **real**
//! panel surface.
//!
//! [`ModulePanel`] at play density is what Studio's play tab shows — the
//! same widgets, the same engaged/read colors, the same detail popups. The
//! docs embed adds exactly two things around it: fan-out and a reset chip.
//!
//! # Fan-out (Q12)
//!
//! `sim=disc,grid` is the article's "one effect, any shape" beat: one row
//! of knobs driving two sims. Display state reads from the **first** named
//! sim (one panel, one set of values); gestures go to **every** named sim.
//!
//! Not every action is safe to fan out, though. Panel gestures are keyed
//! by `(scope, channel)` — structural identity that two deploys of the
//! same module shape share — so `PanelWriteOp` / `PanelClearOp` /
//! `PanelAutoSaveOp` fan out. Anything else a control can raise (a slot
//! edit, whose `ProjectSlotAddress` names one project's handle) goes to
//! the primary sim only, because the same address means nothing in the
//! other sim's session.
//!
//! # Read-only
//!
//! `mode=readonly` renders the identical surface with no dispatchers at
//! all and pointer events off, plus a quiet "Read-only" note in the
//! chrome. Not greyed out and not disabled-looking: the panel is a
//! *picture of a panel* there, and it should look like the real thing,
//! because it is.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, PanelAutoSaveOp, PanelClearOp, PanelWriteOp, ProjectController, ProjectOp,
    UiAction, UiNodeFace, UiPanelGroup, UiStudioView, UiViewContent,
};

use crate::app::module::{ModulePanel, panel_gesture_actions};
use crate::app::node::slot_edit_actions::panel_clear_scope_action;

use super::docs_sims::{DocsSim, DocsSimRegistry};
use super::embed_frame::{EmbedFrame, EmbedLoading, EmbedProblem};

/// How the article asked for the panel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PanelMode {
    /// Knobs drive the sims (the default, and what the article uses).
    #[default]
    Interactive,
    /// A picture of the panel: gestures go nowhere.
    Readonly,
}

impl PanelMode {
    /// Parse a `mode=` argument; unknown words are author error.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "interactive" => Some(Self::Interactive),
            "readonly" => Some(Self::Readonly),
            _ => None,
        }
    }
}

/// Whether an action is keyed by structural `(scope, channel)` identity —
/// and so means the same thing in every sim running the same module shape
/// — or by a session-specific address, which does not travel.
pub(crate) fn fans_out(action: &UiAction) -> bool {
    action.op_as::<PanelWriteOp>().is_some()
        || action.op_as::<PanelClearOp>().is_some()
        || action.op_as::<PanelAutoSaveOp>().is_some()
}

/// The fence, resolved against page context: display state from the first
/// named sim, gestures fanned out to all of them.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PanelEmbed(
    /// The article's `sim=` argument, possibly comma-separated.
    sims: String,
    #[props(default)] mode: PanelMode,
) -> Element {
    let Some(registry) = try_consume_context::<DocsSimRegistry>() else {
        return rsx! {
            DocsPanelSurface { mode }
        };
    };
    let (sims, missing) = registry.resolve(&sims);
    if let Some(name) = missing.first() {
        return rsx! {
            EmbedProblem {
                message: format!("`panel` names sim `{name}`, which this page does not declare."),
            }
        };
    }
    let Some(primary) = sims.first().cloned() else {
        return rsx! {
            EmbedProblem { message: "`panel` resolved to no sims at all.".to_string() }
        };
    };
    // Display state is the PRIMARY sim's: one panel, one set of values.
    let panel = root_panel(&primary.view.read());

    let dispatch_sims = sims.clone();
    let on_action = EventHandler::new(move |action: UiAction| {
        if fans_out(&action) {
            for sim in &dispatch_sims {
                sim.dispatch(action.clone());
            }
        } else if let Some(sim) = dispatch_sims.first() {
            // A session-keyed op only means something in the sim whose
            // panel produced it.
            sim.dispatch(action);
        }
    });
    // Reset puts every named sim back: the article's Reset means "put this
    // page back the way it was", not "put one of these two back".
    let reset_sims = sims;
    let on_reset = EventHandler::new(move |()| {
        for sim in &reset_sims {
            reset_docs_sim(sim);
        }
    });

    rsx! {
        DocsPanelSurface { panel, mode, on_action, on_reset }
    }
}

/// The panel surface in the shared embed chrome.
///
/// `panel` is the root module's group; `None` renders the reserved-height
/// loading state while the sim boots. Stories pass a fixture group
/// directly — no live host anywhere in the path.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DocsPanelSurface(
    /// The root module's panel, once the sim has one.
    #[props(default)]
    panel: Option<UiPanelGroup>,
    #[props(default)] mode: PanelMode,
    /// Where gestures go. Absent (read-only, stories) means nowhere.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
    /// The reset chip; absent hides it.
    #[props(default)]
    on_reset: Option<EventHandler<()>>,
    #[props(default)] caption: Option<String>,
) -> Element {
    let readonly = mode == PanelMode::Readonly;
    let live = if readonly { None } else { on_action };
    let note = match (readonly, panel.is_some()) {
        (true, _) => Some("Read-only".to_string()),
        (false, false) => Some("Loading".to_string()),
        (false, true) => None,
    };
    // Read-only swallows gestures at the DOM edge as well as the dispatch
    // edge: without this a reader can still drag a knob and watch nothing
    // happen, which reads as broken rather than as illustrative.
    let body_class = if readonly {
        "tw:pointer-events-none tw:select-none"
    } else {
        ""
    };

    rsx! {
        EmbedFrame {
            caption,
            note,
            on_reset: if readonly { None } else { on_reset },
            match panel {
                Some(panel) => rsx! {
                    div { class: "{body_class}",
                        ModulePanel {
                            panel,
                            // Panel-state auto-save is the project owner's
                            // switch; a docs sim has no project to own.
                            auto_save: None,
                            play: true,
                            on_panel: live.map(panel_gesture_actions),
                            on_action: live,
                            // R6: the chrome's always-mounted Reset is this
                            // embed's one reset — the panel's transient
                            // glyph would double it and reflow on hold.
                            show_reset: false,
                        }
                    }
                },
                None => rsx! {
                    EmbedLoading {
                        message: "Starting the simulator — the knobs appear here.".to_string(),
                        min_height: 120,
                    }
                },
            }
        }
    }
}

/// The docs Reset, shared by every embed that offers one: "put this page
/// back the way it started" is one gesture, so the panel's chip and the
/// editor's chip must do the identical thing.
///
/// THREE steps, and each earns its place:
///
/// 1. **Clear the root panel scope** — a panel write is runtime state that
///    outlives a redeploy. Verified in the browser during round 1: after a
///    redeploy alone the panel still read "1 held control" and the knob
///    stayed where the reader had dragged it. This is the same clear the
///    panel's own ↺ raises, and it descends into nested groups.
/// 2. **Revert every applied edit** — the editor's auto-apply parks an
///    overlay edit on the artifact, which is likewise runtime state; the
///    project-wide revert is the one gesture that clears them all without
///    the embed having to enumerate artifacts.
/// 3. **Re-deploy the pristine example**, which restores the files (and,
///    through the normal doc-resync path, the editor's text).
///
/// All three ride the same ordered queue, so the order above is the order
/// the controller executes them in.
pub(crate) fn reset_docs_sim(sim: &DocsSim) {
    if let Some(scope) = root_panel(&sim.view.read()).and_then(|panel| panel.target) {
        sim.dispatch(panel_clear_scope_action(scope));
    }
    sim.dispatch(UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::RevertAllEdits,
    ));
    sim.reset();
}

/// The root module's panel from a docs sim's view: the workspace ROOT's
/// group, the same one play mode renders (`docs/design/modules.md` R8).
/// `None` until the sim has deployed and synced.
pub(crate) fn root_panel(studio_view: &UiStudioView) -> Option<UiPanelGroup> {
    studio_view.panes.iter().find_map(|pane| {
        let UiViewContent::ProjectEditor(editor) = &pane.body else {
            return None;
        };
        match editor.nodes.first()?.face.as_ref()? {
            UiNodeFace::Module(face) => Some(face.panel.clone()),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{ControllerId, HomeOp, ProjectController, ProjectOp};

    fn project_action(op: impl lpa_studio_core::ControllerOp) -> UiAction {
        UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op)
    }

    #[test]
    fn mode_arguments_parse_to_the_two_modes() {
        assert_eq!(
            PanelMode::parse("interactive"),
            Some(PanelMode::Interactive)
        );
        assert_eq!(PanelMode::parse("readonly"), Some(PanelMode::Readonly));
        assert_eq!(PanelMode::parse("read-only"), None);
    }

    /// The fan-out gate is the whole reason two sims can share one row of
    /// knobs: `(scope, channel)` travels, a project handle does not.
    #[test]
    fn panel_gestures_fan_out_and_nothing_else_does() {
        assert!(fans_out(&project_action(PanelWriteOp {
            scope: lpc_wire::WireScopeRef::Module {
                owner: lpa_studio_core::NodeId::new(1),
            },
            channel: "scale".to_string(),
            value: lpa_studio_core::LpValue::F32(1.0),
            ttl_ms: None,
        })));
        assert!(fans_out(&project_action(PanelClearOp {
            request: lpc_wire::WirePanelClearRequest::Scope {
                scope: lpc_wire::WireScopeRef::Module {
                    owner: lpa_studio_core::NodeId::new(1),
                },
            },
        })));
        assert!(fans_out(&project_action(PanelAutoSaveOp { enabled: true })));
        // A project-scoped op names one session's document.
        assert!(!fans_out(&project_action(ProjectOp::RefreshProject)));
        assert!(!fans_out(&UiAction::from_op(
            ControllerId::new(lpa_studio_core::HOME_NODE_ID),
            HomeOp::OpenExample {
                id: "examples/plasma".to_string(),
            },
        )));
    }

    #[test]
    fn an_empty_view_has_no_panel_yet() {
        assert!(root_panel(&UiStudioView::empty()).is_none());
    }
}
