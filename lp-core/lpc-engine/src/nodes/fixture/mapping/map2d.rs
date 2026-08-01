//! Resolve authored map2d documents into runtime `PathPoints` mappings.
//!
//! `lpc-mapping` owns the document schema and the deterministic doc-space
//! resolution; this module aspect-fits the resolved lamps into the fixture's
//! texture space and repackages them as one `PathSpec::PointList` per object
//! so the existing precompute and end-to-end channel assignment stay
//! untouched. Both authored sources — `MappingConfig::Map2d` documents and
//! legacy `MappingConfig::SvgPath` imports — funnel through here.

use lp_collection::VecMap;

use lpc_mapping::{Map2dDoc, Map2dError, fit_points, resolve};
use lpc_model::nodes::fixture::{MappingConfig, PathSpec};
use lpc_model::{EnumSlot, MapSlot};

/// Resolve a map2d document into a `PathPoints` mapping for a
/// `texture_width` × `texture_height` fixture.
pub fn mapping_from_map2d_doc(
    doc: &Map2dDoc,
    texture_width: u32,
    texture_height: u32,
) -> Result<MappingConfig, Map2dError> {
    let resolved = resolve(doc)?;
    let mut paths = VecMap::new();
    if !resolved.lamps.is_empty() {
        let fitted = fit_points(
            &resolved.positions(),
            doc.canvas_bounds(),
            texture_width,
            texture_height,
        )?;
        for span in &resolved.spans {
            let start = span.start as usize;
            let end = start + span.count as usize;
            paths.insert(
                span.object,
                EnumSlot::new(PathSpec::point_list(
                    span.start,
                    fitted[start..end].to_vec(),
                )),
            );
        }
    }
    Ok(MappingConfig::path_points(
        MapSlot::new(paths),
        doc.sample_diameter,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::import::svg_to_doc;

    // These expectations are carried over from the legacy in-engine SVG
    // resolver so the lpc-mapping rewrite provably preserves its numbers.

    #[test]
    fn resolves_sorted_svg_groups_to_point_list_paths() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="10 0 20 0"/><text>path:2,count:2</text></g>
  <g><polyline points="0 0 10 0"/><text>path:1,count:3</text></g>
</svg>
"#;
        let doc = svg_to_doc(svg, 2.0).expect("import");
        let mapping = mapping_from_map2d_doc(&doc, 20, 10).expect("resolve");
        let MappingConfig::PathPoints { paths, .. } = mapping else {
            panic!("expected path points");
        };
        assert_eq!(paths.entries.len(), 2);
        let PathSpec::PointList {
            first_channel,
            points,
        } = paths.entries.get(&0).unwrap().value();
        assert_eq!(*first_channel.value(), 0);
        assert_eq!(points.entries.len(), 3);
        let PathSpec::PointList {
            first_channel,
            points,
        } = paths.entries.get(&1).unwrap().value();
        assert_eq!(*first_channel.value(), 3);
        assert_eq!(points.entries.len(), 2);
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
        let MappingConfig::PathPoints { paths, .. } = mapping else {
            panic!("expected path points");
        };
        let PathSpec::PointList { points, .. } = paths.entries.get(&0).unwrap().value();
        assert_eq!(points.entries.get(&0).unwrap().value().0, [0.0, 0.25]);
        assert_eq!(points.entries.get(&1).unwrap().value().0, [1.0, 0.75]);
    }

    #[test]
    fn empty_document_resolves_to_no_paths() {
        let doc = Map2dDoc::new();
        let mapping = mapping_from_map2d_doc(&doc, 16, 16).expect("resolve");
        let MappingConfig::PathPoints { paths, .. } = mapping else {
            panic!("expected path points");
        };
        assert!(paths.entries.is_empty());
    }
}
