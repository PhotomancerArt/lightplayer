//! Render a format-6 fixture's parts into today's compact carrier: the
//! lamps' positions fitted into the fixture's texture, and one carrier span
//! per segment in tree order.
//!
//! **Positions live once.** [`render`] writes every lamp into one
//! exact-size buffer; [`Tree::take_positions`] moves that buffer out and the
//! fit rewrites it in place, with no copy
//! (`docs/reports/2026-09-02-per-lamp-memory-table.md`).
//! The rest of the tree is dropped: the patcher lays out its own for each
//! patch (`parts_patch.rs`).
//!
//! [`Tree::take_positions`]: lpc_mapping::part::Tree::take_positions

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_mapping::part::{Mapping, render};
use lpc_mapping::{Bounds2d, fit_points_in_place, fit_scale};
use lpc_model::nodes::fixture::{ResolvedMappingCompact, ResolvedSpan};

/// Render `mapping` for a `texture_width` × `texture_height` fixture into
/// the carrier. `Err` is the fixture's
/// mapping error, in words (a mapping too big for memory is refused here,
/// never aborted).
pub fn render_parts_carrier(
    mapping: &Mapping,
    texture_width: u32,
    texture_height: u32,
) -> Result<ResolvedMappingCompact, String> {
    let mut tree = render(mapping).map_err(|error| format!("render fixture mapping: {error}"))?;
    let mut points = tree.take_positions();

    let mut spans = Vec::new();
    spans
        .try_reserve_exact(tree.segments().len())
        .map_err(|_| {
            format!(
                "render fixture mapping: {} segments need more memory than this board has free",
                tree.segments().len()
            )
        })?;
    for (index, segment) in tree.segments().iter().enumerate() {
        spans.push(ResolvedSpan {
            object: index as u32,
            // Tree order is the fixture's lamp order: a segment's first lamp
            // IS its first channel in the control product.
            first_channel: segment.lamps.start,
            count: segment.count(),
        });
    }
    drop(tree);

    if points.is_empty() {
        // Nothing to fit (the fit would reject empty bounds).
        return Ok(ResolvedMappingCompact {
            spans,
            points,
            sample_diameter: mapping.sample_diameter,
        });
    }
    let frame = canvas_bounds(mapping);
    let fit = |error| format!("fit fixture mapping: {error}");
    // The diameter is a drawing-space length and rides the same fit as the
    // positions (`docs/defects/2026-08-24-map2d-sample-diameter-unit-mismatch.md`).
    let sample_diameter = mapping.sample_diameter
        * fit_scale(&points, frame, texture_width, texture_height).map_err(fit)?;
    fit_points_in_place(&mut points, frame, texture_width, texture_height).map_err(fit)?;
    Ok(ResolvedMappingCompact {
        spans,
        points,
        sample_diameter,
    })
}

/// The mapping's canvas as the fit reads it; a canvas of no width or height
/// is none (as a map2d document's is), and the fit frames the lamps' own
/// bounds.
fn canvas_bounds(mapping: &Mapping) -> Option<Bounds2d> {
    let [min_x, min_y, width, height] = mapping.canvas?;
    (width > f32::EPSILON && height > f32::EPSILON).then_some(Bounds2d {
        min_x,
        min_y,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::fit_points;
    use lpc_mapping::part::{Part, PartId, PartKind, Path, Repeat, Shape};

    /// The carrier holds the tree's positions, fitted, in the render's one
    /// exact-size buffer.
    #[test]
    fn the_carrier_holds_the_trees_positions_fitted() {
        let mapping = mapping(&[3, 5]);
        let raw = render(&mapping).unwrap().positions().to_vec();
        let carrier = render_parts_carrier(&mapping, 40, 20).unwrap();

        let frame = Some(Bounds2d {
            min_x: 0.0,
            min_y: 0.0,
            width: 100.0,
            height: 50.0,
        });
        assert_eq!(carrier.points, fit_points(&raw, frame, 40, 20).unwrap());
        assert_eq!(
            carrier.points.capacity(),
            carrier.points.len(),
            "the render's one exact-size buffer"
        );
        assert_eq!(
            carrier.sample_diameter,
            2.0 * 0.4,
            "the diameter rode the fit"
        );
    }

    /// One carrier span per segment, in tree order.
    #[test]
    fn one_carrier_span_per_segment_in_tree_order() {
        let carrier = render_parts_carrier(&mapping(&[3, 5, 2]), 40, 20).unwrap();
        let spans: Vec<(u32, u32, u32)> = carrier
            .spans
            .iter()
            .map(|s| (s.object, s.first_channel, s.count))
            .collect();
        assert_eq!(spans, [(0, 0, 3), (1, 3, 5), (2, 8, 2)]);
    }

    /// A mapping too big for memory is refused, not aborted.
    #[test]
    fn a_mapping_too_big_for_memory_is_refused_not_aborted() {
        // u32::MAX copies of u32::MAX lamps: more than any heap can hand
        // over in one piece, so the one big ask fails before anything runs.
        let mut huge = mapping(&[]);
        let path = huge.new_part(PartKind::Shape(Shape::Path(Path::through(
            &[[10.0, 5.0], [90.0, 5.0]],
            u32::MAX,
        ))));
        let repeat = huge.new_part(PartKind::Repeat(Repeat::new(path, [50.0, 25.0], u32::MAX)));
        huge.parts.push(repeat);
        let error = render_parts_carrier(&huge, 40, 20).unwrap_err();
        assert!(
            error.contains("more than this board's memory has free"),
            "{error}"
        );
    }

    // --- helpers ---

    /// One straight path per count, side by side, on a 100 × 50 canvas.
    fn mapping(counts: &[u32]) -> Mapping {
        let mut mapping = Mapping::new();
        for (i, count) in counts.iter().enumerate() {
            let y = 5.0 + 10.0 * i as f32;
            let id = PartId::new(&alloc::format!("path{}", i + 1)).unwrap();
            mapping.parts.push(Part::new(
                id,
                PartKind::Shape(Shape::Path(Path::through(&[[10.0, y], [90.0, y]], *count))),
            ));
        }
        mapping.canvas = Some([0.0, 0.0, 100.0, 50.0]);
        mapping.sample_diameter = 2.0;
        mapping
    }
}
