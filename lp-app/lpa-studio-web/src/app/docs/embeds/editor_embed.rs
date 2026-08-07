//! `editor`: the docs sim's shader source, in the **real** inline GLSL
//! editor.
//!
//! [`AssetEditor`] is the same component the shader card's code drawer
//! renders: CodeMirror with the GLSL mode, live user-symbol completions,
//! the gentle two-half status bar, and — the reason this embed exists —
//! the 500 ms debounced **auto-apply**. A reader who edits `13.0` to
//! `26.0` in an article gets exactly what a user editing the same shader
//! in Studio gets, because it is the same code path end to end.
//!
//! # Where the editor data comes from
//!
//! [`lpa_studio_core::UiAssetEditor`] is controller-produced, not
//! embed-assembled: the project controller resolves each asset slot's
//! source against its node's def artifact and hangs the editor data on
//! `UiSlotAsset::inline_editor`; the shader node's face builder lifts the
//! first GLSL one onto `UiShaderFace::code_drawer`. So this embed just
//! walks the docs sim's view for a shader face and takes what is already
//! there — there is no "open the drawer" op to drive, and no docs-only
//! projection.
//!
//! # Where its actions go
//!
//! Straight into the sim's controller, unfiltered. Every action the editor
//! raises is project-scoped and names the artifact it acts on
//! (`AssetContentFetchOp`, `AssetEditOp::ApplyBody`, `AssetEditOp::Revert`,
//! `ProjectOp::SaveOverlay`), so unlike a panel gesture none of it needs a
//! fan-out gate — there is one sim and the addresses are its own.
//!
//! Content resolution needs no help either: the editor dispatches its own
//! `fetch_action()` the first time it renders with `content == None`, and
//! the refreshed view carries the text back. The docs host is a real
//! `StudioController`, so that round trip works exactly as it does in the
//! app.
//!
//! # Height
//!
//! The real editor is already fixed-height (a 2 rem status bar over an
//! 18 rem CodeMirror that scrolls internally), so the embed is a stable
//! 20 rem and the loading box reserves exactly that. Nothing here adds an
//! `overflow` clip: the compile-error popover and the completion popup
//! both escape the editor's box on purpose.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiAssetEditor as UiAssetEditorData, UiNodeChild, UiNodeFace, UiNodeView,
    UiStudioView, UiViewContent,
};

use crate::app::node::AssetEditor;
use crate::base::Platform;

use super::docs_sims::DocsSimRegistry;
use super::embed_frame::{EmbedFrame, EmbedLoading, EmbedProblem};
use super::panel_embed::reset_docs_sim;

/// Reserved height while the sim boots: the status bar (`h-8`, 32px) plus
/// the editor body (`h-72`, 288px) the live surface will occupy.
const EDITOR_HEIGHT: u32 = 320;

/// The fence, resolved against page context.
///
/// No provider (the story book, host builds) renders the same calm loading
/// box a booting sim does — an article never shows an editor it cannot
/// wire up.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn EditorEmbed(
    /// The article's `sim=` handle.
    sim: String,
) -> Element {
    let Some(registry) = try_consume_context::<DocsSimRegistry>() else {
        return rsx! {
            DocsEditorSurface {}
        };
    };
    let Some(entry) = registry.get(&sim) else {
        return rsx! {
            EmbedProblem {
                message: format!("`editor` names sim `{sim}`, which this page does not declare."),
            }
        };
    };
    let editor = shader_editor(&entry.view.read());

    let dispatch_sim = entry.clone();
    let on_action = EventHandler::new(move |action: UiAction| dispatch_sim.dispatch(action));
    // The same Reset the panel offers, and deliberately the same gesture:
    // one "put this page back" that restores the knobs AND the source.
    let reset_sim = entry.clone();
    let on_reset = EventHandler::new(move |()| reset_docs_sim(&reset_sim));

    rsx! {
        DocsEditorSurface { editor, on_action, on_reset }
    }
}

/// The editor surface in the shared embed chrome.
///
/// `editor` is the resolved [`UiAssetEditorData`]; `None` means the sim has
/// not produced a shader face yet, which renders the reserved-height
/// loading state. Stories pass a fixture straight in — that is the seam
/// that keeps live workers out of the story book.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DocsEditorSurface(
    /// The shader's editor data, once the sim has a shader face.
    #[props(default)]
    editor: Option<UiAssetEditorData>,
    /// Where the editor's actions go. Absent (stories) means nowhere, and
    /// the editor's own content fetch quietly never fires.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
    /// The reset chip; absent hides it.
    #[props(default)]
    on_reset: Option<EventHandler<()>>,
    #[props(default)] caption: Option<String>,
    /// Platform for the editor's shortcut hints; stories pin it.
    #[props(default)]
    platform: Option<Platform>,
) -> Element {
    rsx! {
        EmbedFrame {
            caption,
            note: if editor.is_none() { Some("Loading".to_string()) } else { None },
            on_reset,
            match editor {
                Some(editor) => rsx! {
                    AssetEditor { editor, on_action, platform }
                },
                None => rsx! {
                    EmbedLoading {
                        message: "Starting the simulator — the shader's source appears here."
                            .to_string(),
                        min_height: EDITOR_HEIGHT,
                    }
                },
            }
        }
    }
}

/// The first shader face's code drawer anywhere in the lensed session's
/// node tree.
///
/// "First shader" is the right rule for the docs shape and says so: a docs
/// example is one effect being explained, so a page that grows a second
/// shader wants a `node=` argument here, not a silent pick.
pub(crate) fn shader_editor(studio_view: &UiStudioView) -> Option<UiAssetEditorData> {
    studio_view.panes.iter().find_map(|pane| {
        let UiViewContent::ProjectEditor(editor) = &pane.body else {
            return None;
        };
        editor.nodes.iter().find_map(node_editor)
    })
}

/// Depth-first over a workspace node and everything under it.
fn node_editor(node: &UiNodeView) -> Option<UiAssetEditorData> {
    face_editor(node.face.as_ref()).or_else(|| node.children.iter().find_map(child_editor))
}

/// Depth-first over a nested child card and everything under it.
fn child_editor(child: &UiNodeChild) -> Option<UiAssetEditorData> {
    face_editor(child.face.as_ref()).or_else(|| child.children.iter().find_map(child_editor))
}

/// The GLSL editor a face carries. Only a shader face has one — a
/// fixture's mapping editor is a different asset kind and not what
/// `editor` means in an article about shaders.
fn face_editor(face: Option<&UiNodeFace>) -> Option<UiAssetEditorData> {
    match face? {
        UiNodeFace::Shader(face) => face.code_drawer.clone(),
        UiNodeFace::Module(_)
        | UiNodeFace::Fixture(_)
        | UiNodeFace::Output(_)
        | UiNodeFace::Playlist(_)
        | UiNodeFace::Clock(_)
        | UiNodeFace::Controls(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view with nothing in it (the sim is still booting) resolves no
    /// editor rather than panicking — that is the loading state's input.
    #[test]
    fn an_empty_view_has_no_editor_yet() {
        assert!(shader_editor(&UiStudioView::empty()).is_none());
    }

    #[test]
    fn a_faceless_node_carries_no_editor() {
        assert!(face_editor(None).is_none());
    }
}
