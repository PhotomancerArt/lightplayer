//! The overlay for the view tree: what's drawn over the cards at one size
//! on screen, in screen pixels. Outlines are areas (`area.rs`), the same
//! shapes a click hits, so what's outlined is what a click selects.

use crate::domain::component::{Component, ComponentId};
use crate::domain::geom::{Rect, Vec2};
use crate::domain::object::objects_of;
use crate::domain::target::Target;
use crate::ux::editor::Editor;
use crate::ux::ui::{OutlineKind, UiDraftObject, UiHandle, UiOutline, UiOverlay, UiShape};
use crate::ux::view_faces::direction_arrow;

impl Editor {
    pub fn view_overlay(&self) -> UiOverlay {
        let p = &self.project;
        let cam = &self.camera;
        let mut outlines: Vec<UiOutline> = Vec::new();
        let mut add = |t: &Target, kind: OutlineKind, label: bool| {
            if let Some(a) = p.area(cam, t) {
                let top = a.top();
                outlines.push(UiOutline {
                    path: a.outline_path(),
                    stroke: a.stroke_width(),
                    kind,
                    label: label.then(|| (p.label(t), top)),
                });
            }
        };
        let context = self.context();
        if let Some(c) = &context {
            // A node's title already names it; a port's row too.
            add(
                c,
                OutlineKind::Context,
                !matches!(c, Target::Node(_) | Target::Port { .. }),
            );
        }
        for t in &self.selection {
            // A node's title already names it; a port's row too; a pixel is
            // named in the inspector.
            let label = !matches!(
                t,
                Target::Lamp { .. } | Target::Node(_) | Target::Port { .. } | Target::Pixel { .. }
            );
            add(t, OutlineKind::Selected, label);
        }
        // In the right-click list, the item under the pointer is outlined;
        // otherwise, what a click would select.
        let menu_hover = self
            .menu
            .as_ref()
            .and_then(|m| m.hover.and_then(|i| m.items.get(i)))
            .map(|i| i.target.clone());
        if let Some(h) = menu_hover.or_else(|| self.hover_target())
            && !self.selection.contains(&h)
        {
            add(&h, OutlineKind::Hover, false);
        }

        let hovered = self.hover_handle().map(|h| h.kind);
        let dragged = self.dragged_handle().cloned();
        let handles = self
            .handles()
            .into_iter()
            .map(|h| UiHandle {
                rect: h.rect(cam),
                on: Some(&h.kind) == hovered.as_ref() || Some(&h.kind) == dragged.as_ref(),
            })
            .collect();

        // The drawing in progress, in the fixture it starts in.
        let bounds = self.draft_fixture().and_then(|id| p.node_rect(&id));
        let draft = self
            .draft()
            .map(|kind| {
                objects_of(&Component {
                    id: ComponentId::new("draft"),
                    kind,
                })
                .iter()
                .map(|o| {
                    let shape = match &o.shape {
                        crate::domain::object::ObjectShape::Polyline { points } => {
                            UiShape::Polyline(points.iter().map(|q| cam.to_screen(*q)).collect())
                        }
                        crate::domain::object::ObjectShape::Ring { center, radius } => {
                            UiShape::Ring {
                                center: cam.to_screen(*center),
                                radius: radius * cam.scale,
                            }
                        }
                    };
                    let lamps: Vec<(Vec2, bool)> = o
                        .lamps
                        .iter()
                        .map(|l| {
                            let inside = bounds.is_some_and(|b| b.contains(l.pos));
                            (cam.to_screen(l.pos), inside)
                        })
                        .collect();
                    let at: Vec<Vec2> = lamps.iter().map(|(q, _)| *q).collect();
                    UiDraftObject {
                        arrow: direction_arrow(&shape, &at),
                        shape,
                        lamps,
                    }
                })
                .collect()
            })
            .unwrap_or_default();

        UiOverlay {
            outlines,
            handles,
            selection_box: self
                .selection_box()
                .map(|r| Rect::from_corners(cam.to_screen(r.min), cam.to_screen(r.max))),
            draft,
            draft_points: self.path_draft.iter().map(|q| cam.to_screen(*q)).collect(),
            marked_lamps: self
                .marked_lamps()
                .iter()
                .filter_map(|l| p.lamps_under(l).first().map(|q| cam.to_screen(*q)))
                .collect(),
        }
    }
}
