//! The Graph's pictures (plan D11): a shader's live picture on its card,
//! and the picture a fixture samples behind its lamps.
//!
//! They follow Studio's own probe rules, sharpened, not replaced
//! (`ProjectController::node_subscribes_products`): on a simulator lens a
//! picture is cheap, so every render source on the Graph shows one; on a
//! device lens every picture is a render on the board and frames over its
//! link, so only what the selection asks for does: a selected source, and
//! the source a selected fixture (or anything in one) samples. Nothing is
//! asked for while the Graph isn't the place.

use std::collections::BTreeMap;
use std::sync::Arc;

use lab_mapping_model::{BoardFrame, Editor, NodeId, Picture, Target};

use crate::UiProductRef;
use crate::app::project::ProjectController;

/// The render sources whose pictures the Graph shows now.
pub(crate) fn wanted_sources(editor: &Editor, device: bool) -> Vec<NodeId> {
    let p = &editor.project;
    let mut out: Vec<NodeId> = Vec::new();
    let mut add = |id: NodeId| {
        if !out.contains(&id) {
            out.push(id);
        }
    };
    if !device {
        for n in p.all_nodes() {
            if n.source().is_some() {
                add(n.id.clone());
            }
        }
        return out;
    }
    for t in &editor.selection {
        match t {
            Target::Node(id) if p.node(id).is_some_and(|n| n.source().is_some()) => add(id.clone()),
            other => {
                if let Some(source) = p.fixture_of_target(other).and_then(|f| p.visual_source(&f)) {
                    add(source);
                }
            }
        }
    }
    out
}

/// Each wanted source's visual product (`nodes`: the lab's nodes as
/// Studio's).
pub(crate) fn wanted_products(
    project: &ProjectController,
    nodes: &BTreeMap<NodeId, lpc_model::NodeId>,
    sources: &[NodeId],
) -> Vec<(NodeId, UiProductRef)> {
    sources
        .iter()
        .filter_map(|id| Some((id, *nodes.get(id)?)))
        .flat_map(|(id, node)| {
            project
                .graph_visual_products(node)
                .into_iter()
                .take(1)
                .map(move |product| (id.clone(), product))
        })
        .collect()
}

/// `frame` with every wanted source's latest picture.
pub(crate) fn with_pictures(
    mut frame: BoardFrame,
    project: &ProjectController,
    wanted: &[(NodeId, UiProductRef)],
) -> BoardFrame {
    for (source, product) in wanted {
        let Some((width, height, bytes)) = project.graph_picture(product) else {
            continue;
        };
        if let Some(picture) = Picture::new(width, height, Arc::from(&*bytes)) {
            frame = frame.with_picture(source.clone(), picture);
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_mapping_model::Project;

    /// On a simulator lens every render source shows its picture; on a
    /// device lens none does until the selection asks, and then only the
    /// one the selected fixture samples.
    #[test]
    fn a_simulator_shows_every_picture_and_a_device_the_selections() {
        let mut editor = Editor::new(Project::seed());
        let sources: Vec<NodeId> = editor
            .project
            .all_nodes()
            .iter()
            .filter(|n| n.source().is_some())
            .map(|n| n.id.clone())
            .collect();
        assert!(!sources.is_empty());
        assert_eq!(wanted_sources(&editor, false), sources);
        assert!(wanted_sources(&editor, true).is_empty());
        let fixture = editor.project.fixtures()[0].id.clone();
        let feeding = editor.project.visual_source(&fixture);
        editor.selection = vec![Target::Node(fixture)];
        assert_eq!(
            wanted_sources(&editor, true),
            feeding.into_iter().collect::<Vec<_>>()
        );
    }
}
