//! Tiny SVG path mapping importer.

mod svg_path_data;
mod svg_path_error;
mod svg_path_fit;
mod svg_path_group;
mod svg_path_parser;

use alloc::collections::BTreeMap;

use lpc_model::nodes::fixture::{MappingConfig, PathSpec};
use lpc_model::{EnumSlot, MapSlot};

pub use svg_path_error::SvgPathError;
pub use svg_path_group::{SvgPathGeometry, SvgPathGroup};
pub use svg_path_parser::parse_svg_path_groups;

use svg_path_fit::fit_points;

pub fn resolve_svg_path_mapping(
    svg: &str,
    texture_width: u32,
    texture_height: u32,
    sample_diameter: f32,
) -> Result<MappingConfig, SvgPathError> {
    let mut parsed = parse_svg_path_groups(svg)?;
    parsed.groups.sort_by_key(|group| group.path_index);

    let mut paths = BTreeMap::new();
    let mut first_channel = 0u32;
    let mut previous_path_index = None;
    for group in &parsed.groups {
        if previous_path_index == Some(group.path_index) {
            return Err(SvgPathError::DuplicatePathIndex(group.path_index));
        }
        previous_path_index = Some(group.path_index);

        let sampled = group.sample_points()?;
        let fitted = fit_points(&sampled, parsed.view_box, texture_width, texture_height)?;
        paths.insert(
            group.path_index,
            EnumSlot::new(PathSpec::point_list(first_channel, fitted)),
        );
        first_channel = first_channel.saturating_add(group.count);
    }

    if paths.is_empty() {
        return Err(SvgPathError::NoMappingGroups);
    }

    Ok(MappingConfig::path_points(
        MapSlot::new(paths),
        sample_diameter,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::nodes::fixture::PathSpec;

    #[test]
    fn resolves_sorted_groups_to_point_list_paths() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="10 0 20 0"/><text>path:2,count:2</text></g>
  <g><polyline points="0 0 10 0"/><text>path:1,count:3</text></g>
</svg>
"#;
        let mapping = resolve_svg_path_mapping(svg, 20, 10, 2.0).expect("resolve");
        let MappingConfig::PathPoints { paths, .. } = mapping else {
            panic!("expected path points");
        };
        assert_eq!(paths.entries.len(), 2);
        let PathSpec::PointList {
            first_channel,
            points,
        } = paths.entries.get(&1).unwrap().value()
        else {
            panic!("expected point list");
        };
        assert_eq!(*first_channel.value(), 0);
        assert_eq!(points.entries.len(), 3);
        let PathSpec::PointList {
            first_channel,
            points,
        } = paths.entries.get(&2).unwrap().value()
        else {
            panic!("expected point list");
        };
        assert_eq!(*first_channel.value(), 3);
        assert_eq!(points.entries.len(), 2);
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
            resolve_svg_path_mapping(svg, 20, 10, 2.0),
            Err(SvgPathError::DuplicatePathIndex(1))
        ));
    }

    #[test]
    fn fits_wide_view_box_into_square_without_stretching() {
        let svg = r#"
<svg viewBox="0 0 20 10">
  <g><polyline points="0 0 20 10"/><text>path:1,count:2</text></g>
</svg>
"#;
        let mapping = resolve_svg_path_mapping(svg, 10, 10, 2.0).expect("resolve");
        let MappingConfig::PathPoints { paths, .. } = mapping else {
            panic!("expected path points");
        };
        let PathSpec::PointList { points, .. } = paths.entries.get(&1).unwrap().value() else {
            panic!("expected point list");
        };
        assert_eq!(points.entries.get(&0).unwrap().value().0, [0.0, 0.25]);
        assert_eq!(points.entries.get(&1).unwrap().value().0, [1.0, 0.75]);
    }
}
