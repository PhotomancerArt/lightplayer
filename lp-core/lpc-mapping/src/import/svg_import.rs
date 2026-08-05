//! SVG-subset → mapping-document conversion.
//!
//! Understands the strict Illustrator-friendly subset the fyeah sign uses:
//! top-level `<g>` groups holding exactly one straight-line `<path>`/
//! `<polyline>` plus a `path:N,count:N` text label. Groups become [`PathShape`]
//! objects sorted by `N`; the root `viewBox` becomes the document canvas so a
//! fitted layout survives conversion.

use alloc::format;
use alloc::vec::Vec;

use crate::map2d_doc::{Map2dDoc, Map2dObject, Map2dShape, PathShape};

use super::svg_error::SvgImportError;
use super::svg_group::SvgPathGeometry;
use super::svg_parser::parse_svg_path_groups;

/// Convert a mapping SVG into a document. `sample_diameter` seeds the
/// doc-level default (the SVG itself does not carry one).
pub fn svg_to_doc(svg: &str, sample_diameter: f32) -> Result<Map2dDoc, SvgImportError> {
    let mut parsed = parse_svg_path_groups(svg)?;
    parsed.groups.sort_by_key(|group| group.path_index);

    let mut objects = Vec::with_capacity(parsed.groups.len());
    let mut previous_path_index = None;
    for group in parsed.groups {
        if previous_path_index == Some(group.path_index) {
            return Err(SvgImportError::DuplicatePathIndex(group.path_index));
        }
        previous_path_index = Some(group.path_index);

        let SvgPathGeometry::Polyline(points) = group.geometry;
        objects.push(Map2dObject {
            name: format!("p{}", group.path_index),
            shape: Map2dShape::Path(PathShape {
                points,
                count: group.count,
                reversed: false,
                gaps: Vec::new(),
            }),
        });
    }
    if objects.is_empty() {
        return Err(SvgImportError::NoMappingGroups);
    }

    Ok(Map2dDoc {
        sample_diameter,
        canvas: parsed
            .view_box
            .map(|b| [b.min_x, b.min_y, b.width, b.height]),
        objects,
        ..Map2dDoc::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map2d_resolve::resolve;

    #[test]
    fn converts_sorted_groups_to_path_objects() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="10 0 20 0"/><text>path:2,count:2</text></g>
  <g><polyline points="0 0 10 0"/><text>path:1,count:3</text></g>
</svg>
"#;
        let doc = svg_to_doc(svg, 2.0).unwrap();
        assert_eq!(doc.canvas, Some([0.0, 0.0, 20.0, 10.0]));
        assert_eq!(doc.objects.len(), 2);
        assert_eq!(doc.objects[0].name, "p1");
        assert_eq!(doc.objects[1].name, "p2");
        let resolved = resolve(&doc).unwrap();
        assert_eq!(resolved.spans[0].count, 3);
        assert_eq!(resolved.spans[1].start, 3);
    }

    #[test]
    fn rejects_duplicate_path_indexes() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="0 0 10 0"/><text>path:1,count:2</text></g>
  <g><polyline points="10 0 20 0"/><text>path:1,count:2</text></g>
</svg>
"#;
        assert!(matches!(
            svg_to_doc(svg, 2.0),
            Err(SvgImportError::DuplicatePathIndex(1))
        ));
    }

    #[test]
    fn rejects_svg_without_mapping_groups() {
        assert!(matches!(
            svg_to_doc(
                r#"<svg viewBox="0 0 1 1"><g><path d="M0,0 L1,1"/></g></svg>"#,
                2.0
            ),
            Err(SvgImportError::NoMappingGroups)
        ));
    }
}
