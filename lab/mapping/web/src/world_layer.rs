//! The world layer: one `div` whose transform is the camera, holding every
//! node as a flat sibling. Panning and zooming change only this transform;
//! the cards inside never render for it. Cards outside the view, plus a
//! margin, are not mounted.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lab_mapping_model::{BoardFrame, Camera, Editor, NodeId, Rect, UiCanvasNode, Vec2};

use crate::dense_canvas::zoom_step;
use crate::node_card::NodeCard;

/// Cards this far (screen pixels) outside the view stay mounted, so a short
/// pan shows no gap.
const CULL_MARGIN_PX: f64 = 300.0;

#[component]
pub fn WorldLayer(
    nodes: Vec<UiCanvasNode>,
    rects: Memo<BTreeMap<NodeId, Rect>>,
    camera: Camera,
    viewport: Vec2,
    revision: u64,
    frame: Memo<BoardFrame>,
    ed: Signal<Editor>,
) -> Element {
    let rects = rects.read();
    let zoom = zoom_step(camera.scale);
    let margin = Vec2::new(CULL_MARGIN_PX, CULL_MARGIN_PX);
    let view = Rect::new(
        camera.to_world(Vec2::new(0.0, 0.0) - margin),
        camera.to_world(viewport + margin),
    );
    let visible = |r: &Rect| {
        r.max.x >= view.min.x && r.min.x <= view.max.x && r.max.y >= view.min.y && r.min.y <= view.max.y
    };
    let shown: Vec<(UiCanvasNode, Rect)> = nodes
        .into_iter()
        .filter_map(|n| {
            let r = *rects.get(&n.id)?;
            visible(&r).then_some((n, r))
        })
        .collect();
    let style = format!(
        "transform: translate({}px, {}px) scale({}); --zoom: {}",
        camera.offset.x, camera.offset.y, camera.scale, camera.scale
    );
    rsx! {
        div { class: "world", style: "{style}",
            for (node, rect) in shown {
                NodeCard { key: "{node.id}", node, rect, zoom, revision, frame, ed }
            }
        }
    }
}
