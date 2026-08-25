//! Resolve authored map2d documents into the compact runtime mapping carrier.
//!
//! `lpc-mapping` owns the document schema and the deterministic doc-space
//! resolution; this module aspect-fits the resolved lamps into the fixture's
//! texture space and hands them on as-is. Both authored sources —
//! `MappingConfig::Map2d` documents and legacy `MappingConfig::SvgPath`
//! imports — funnel through here.
//!
//! This used to expand the resolver's already-compact output (positions +
//! one span per physical strand) into `MappingConfig::PathPoints` slots: 24 B of slot
//! tuple per lamp, 41 B/LED live once `VecMap`'s power-of-two capacity
//! overshoot is counted, to carry 8 B of coordinate. Nothing downstream
//! needed the slot addressing — every consumer goes through the mapping
//! point visitor or the path spans — so the expansion is gone and the
//! resolved geometry stays compact. Hand-authored `PathPoints` fixtures are
//! untouched and keep the slot form.

use alloc::vec::Vec;

use lpc_mapping::{Map2dDoc, Map2dError, ResolvedMap2d, fit_points, fit_scale, resolve};
use lpc_model::nodes::fixture::{ResolvedMappingCompact, ResolvedSpan};

/// Resolve a map2d document into the compact mapping carrier for a
/// `texture_width` × `texture_height` fixture.
pub fn mapping_from_map2d_doc(
    doc: &Map2dDoc,
    texture_width: u32,
    texture_height: u32,
) -> Result<ResolvedMappingCompact, Map2dError> {
    let ResolvedMap2d { lamps, spans } = resolve(doc)?;
    if lamps.is_empty() {
        // No geometry to fit (and `fit_points` would reject empty bounds):
        // an empty carrier, matching what an empty document always produced.
        return Ok(ResolvedMappingCompact {
            spans: Vec::new(),
            points: Vec::new(),
            sample_diameter: doc.sample_diameter,
        });
    }

    let positions: Vec<[f32; 2]> = lamps.iter().map(|lamp| lamp.pos).collect();
    // The doc's sample_diameter is a doc-space LENGTH; the carrier's is
    // texture pixels (that is what the sampling footprint and the display
    // layout's lamp radius read). It must ride the same fit as the
    // positions — passed through unscaled it wore whatever unit the doc
    // chose as pixels, which the old preview's absolute clamps masked
    // (docs/defects/2026-08-24-map2d-sample-diameter-unit-mismatch.md).
    let sample_diameter = doc.sample_diameter
        * fit_scale(
            &positions,
            doc.canvas_bounds(),
            texture_width,
            texture_height,
        )?;
    // The 20 B/lamp resolver output has nothing left to say once the
    // positions are copied out; dropping it here keeps the load-time peak to
    // two 8 B/lamp buffers instead of three.
    drop(lamps);
    let points = fit_points(
        &positions,
        doc.canvas_bounds(),
        texture_width,
        texture_height,
    )?;
    drop(positions);

    // One carrier span per resolver span, and the resolver's spans are
    // *strands*, not objects: a `repeat` object emits one per instance, all
    // carrying its object index. Copying the list straight through is what
    // keeps N rotated instances N separate runs downstream — the fixture's
    // honest spans and the output face's strip boundaries both read this.
    let mut compact_spans = Vec::with_capacity(spans.len());
    for span in &spans {
        compact_spans.push(ResolvedSpan {
            object: span.object,
            // Wiring order is the channel order: a strand's first lamp
            // index IS its first channel (what the slot form stored as the
            // point list's `first_channel`).
            first_channel: span.start,
            count: span.count,
        });
    }
    Ok(ResolvedMappingCompact {
        spans: compact_spans,
        points,
        sample_diameter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::import::svg_to_doc;

    // These expectations are carried over from the legacy in-engine SVG
    // resolver so the lpc-mapping rewrite provably preserves its numbers.

    #[test]
    fn resolves_sorted_svg_groups_to_spans() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="10 0 20 0"/><text>path:2,count:2</text></g>
  <g><polyline points="0 0 10 0"/><text>path:1,count:3</text></g>
</svg>
"#;
        let doc = svg_to_doc(svg, 2.0).expect("import");
        let mapping = mapping_from_map2d_doc(&doc, 20, 10).expect("resolve");
        assert_eq!(mapping.spans.len(), 2);
        assert_eq!(mapping.spans[0].object, 0);
        assert_eq!(mapping.spans[0].first_channel, 0);
        assert_eq!(mapping.spans[0].count, 3);
        assert_eq!(mapping.spans[1].object, 1);
        assert_eq!(mapping.spans[1].first_channel, 3);
        assert_eq!(mapping.spans[1].count, 2);
        assert_eq!(mapping.points.len(), 5);
        assert_eq!(mapping.sample_diameter, 2.0);
    }

    /// A repeated document must bridge as N runs, not one: the carrier keeps
    /// one span per instance, each starting where the last ended, and every
    /// one still naming the single document object it came from.
    #[test]
    fn a_repeated_document_bridges_one_span_per_instance() {
        let doc = lpc_mapping::corpus::repeated_sector();
        let mapping = mapping_from_map2d_doc(&doc, 64, 64).expect("resolve");
        assert_eq!(doc.objects.len(), 1);
        assert_eq!(mapping.spans.len(), 5, "five instances, five strands");
        for (instance, span) in mapping.spans.iter().enumerate() {
            assert_eq!(span.object, 0);
            assert_eq!(span.count, 12);
            assert_eq!(span.first_channel, instance as u32 * 12);
        }
        assert_eq!(mapping.points.len(), 60);
        assert_eq!(mapping.lamp_count(), 60);
        // The carrier's invariant holds with repeated spans too.
        assert_eq!(
            mapping
                .spans
                .iter()
                .map(|s| s.count as usize)
                .sum::<usize>(),
            mapping.points.len()
        );
    }

    /// The defect pin: a doc-space diameter must shrink by the same fit
    /// as the positions. A 20-unit-wide doc into a 10 px texture halves
    /// lengths; the diameter is a length.
    #[test]
    fn sample_diameter_rides_the_position_fit() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="0 0 20 10"/><text>path:1,count:2</text></g>
</svg>
"#;
        let doc = svg_to_doc(svg, 2.0).expect("import");
        let mapping = mapping_from_map2d_doc(&doc, 10, 10).expect("resolve");
        assert!((mapping.sample_diameter - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fits_wide_view_box_into_square_without_stretching() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="0 0 20 10"/><text>path:1,count:2</text></g>
</svg>
"#;
        let doc = svg_to_doc(svg, 2.0).expect("import");
        let mapping = mapping_from_map2d_doc(&doc, 10, 10).expect("resolve");
        assert_eq!(mapping.points[0], [0.0, 0.25]);
        assert_eq!(mapping.points[1], [1.0, 0.75]);
    }

    #[test]
    fn empty_document_resolves_to_an_empty_carrier() {
        let doc = Map2dDoc::new();
        let mapping = mapping_from_map2d_doc(&doc, 16, 16).expect("resolve");
        assert!(mapping.spans.is_empty());
        assert!(mapping.points.is_empty());
    }

    /// The carrier is exact-capacity by construction — that is the whole
    /// point of it (8 B/lamp, no `VecMap` power-of-two overshoot).
    #[test]
    fn carrier_buffers_are_exact_capacity() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="10 0 20 0"/><text>path:2,count:2</text></g>
  <g><polyline points="0 0 10 0"/><text>path:1,count:3</text></g>
</svg>
"#;
        let doc = svg_to_doc(svg, 2.0).expect("import");
        let mapping = mapping_from_map2d_doc(&doc, 20, 10).expect("resolve");
        assert_eq!(mapping.points.capacity(), mapping.points.len());
        assert_eq!(mapping.spans.capacity(), mapping.spans.len());
    }
}
