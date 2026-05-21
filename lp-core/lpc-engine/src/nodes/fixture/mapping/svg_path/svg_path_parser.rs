use alloc::string::String;
use alloc::vec::Vec;

use super::svg_path_data::{parse_path_data, parse_polyline};
use super::svg_path_error::{SvgPathError, invalid_label, invalid_number};
use super::svg_path_group::{ParsedSvgPathGroups, SvgBounds, SvgPathGroup};

pub fn parse_svg_path_groups(svg: &str) -> Result<ParsedSvgPathGroups, SvgPathError> {
    let view_box = parse_view_box(svg)?;
    let text_nodes = collect_text_nodes(svg)?;
    let groups = collect_groups(svg)?;
    let mut mapping_groups = Vec::new();
    let mut covered_text = Vec::new();

    for group in groups {
        let group_text_nodes = text_nodes
            .iter()
            .filter(|text| text.start >= group.body_start && text.end <= group.body_end)
            .collect::<Vec<_>>();
        let mapping_texts = group_text_nodes
            .iter()
            .filter(|text| text.normalized.starts_with("path:"))
            .collect::<Vec<_>>();

        if mapping_texts.is_empty() {
            continue;
        }
        if group.body.contains("<g") {
            return Err(SvgPathError::NestedGroup);
        }
        if group_text_nodes.len() != 1 {
            return Err(SvgPathError::MultipleTextElements);
        }

        let label = parse_label(&mapping_texts[0].normalized)?;
        let path_like = collect_path_like_elements(group.body, label.path_index)?;
        mapping_groups.push(SvgPathGroup {
            path_index: label.path_index,
            count: label.count,
            geometry: path_like,
        });
        covered_text.push((mapping_texts[0].start, mapping_texts[0].end));
    }

    for text in text_nodes {
        if text.normalized.starts_with("path:")
            && !covered_text
                .iter()
                .any(|(start, end)| *start == text.start && *end == text.end)
        {
            return Err(SvgPathError::UngroupedMappingText(text.normalized));
        }
    }

    Ok(ParsedSvgPathGroups {
        groups: mapping_groups,
        view_box,
    })
}

struct SvgGroup<'a> {
    body: &'a str,
    body_start: usize,
    body_end: usize,
}

#[derive(Clone)]
struct TextNode {
    normalized: String,
    start: usize,
    end: usize,
}

struct PathLabel {
    path_index: u32,
    count: u32,
}

fn parse_view_box(svg: &str) -> Result<Option<SvgBounds>, SvgPathError> {
    let Some(svg_start) = find_tag(svg, "svg", 0) else {
        return Ok(None);
    };
    let Some(tag_end) = svg[svg_start..].find('>').map(|offset| svg_start + offset) else {
        return Ok(None);
    };
    let Some(value) = attr_value(&svg[svg_start..=tag_end], "viewBox") else {
        return Ok(None);
    };
    let values = value
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if values.len() != 4 {
        return Err(SvgPathError::InvalidViewBox);
    }
    let min_x = parse_f32(values[0])?;
    let min_y = parse_f32(values[1])?;
    let width = parse_f32(values[2])?;
    let height = parse_f32(values[3])?;
    if width <= f32::EPSILON || height <= f32::EPSILON {
        return Err(SvgPathError::InvalidViewBox);
    }
    Ok(Some(SvgBounds {
        min_x,
        min_y,
        width,
        height,
    }))
}

fn collect_groups(svg: &str) -> Result<Vec<SvgGroup<'_>>, SvgPathError> {
    let mut groups = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_tag(svg, "g", cursor) {
        let open_end = svg[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or(SvgPathError::InvalidAttribute { name: "g" })?;
        let close_start = svg[open_end..]
            .find("</g>")
            .map(|offset| open_end + offset)
            .ok_or(SvgPathError::InvalidAttribute { name: "g" })?;
        groups.push(SvgGroup {
            body: &svg[open_end..close_start],
            body_start: open_end,
            body_end: close_start,
        });
        cursor = close_start + "</g>".len();
    }
    Ok(groups)
}

fn collect_text_nodes(svg: &str) -> Result<Vec<TextNode>, SvgPathError> {
    let mut nodes = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_tag(svg, "text", cursor) {
        let open_end = svg[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or(SvgPathError::InvalidAttribute { name: "text" })?;
        let close_start = svg[open_end..]
            .find("</text>")
            .map(|offset| open_end + offset)
            .ok_or(SvgPathError::InvalidAttribute { name: "text" })?;
        let raw = &svg[open_end..close_start];
        nodes.push(TextNode {
            normalized: normalize_text_payload(raw),
            start,
            end: close_start + "</text>".len(),
        });
        cursor = close_start + "</text>".len();
    }
    Ok(nodes)
}

fn collect_path_like_elements(
    group: &str,
    path_index: u32,
) -> Result<super::SvgPathGeometry, SvgPathError> {
    let mut parsed = Vec::new();
    let mut cursor = 0usize;
    while let Some(start) = find_tag(group, "path", cursor) {
        let tag_end = group[start..]
            .find('>')
            .map(|offset| start + offset)
            .ok_or(SvgPathError::InvalidAttribute { name: "path" })?;
        let tag = &group[start..=tag_end];
        let d = attr_value(tag, "d").ok_or(SvgPathError::InvalidAttribute { name: "d" })?;
        parsed.push(parse_path_data(d)?);
        cursor = tag_end + 1;
    }

    cursor = 0;
    while let Some(start) = find_tag(group, "polyline", cursor) {
        let tag_end = group[start..]
            .find('>')
            .map(|offset| start + offset)
            .ok_or(SvgPathError::InvalidAttribute { name: "polyline" })?;
        let tag = &group[start..=tag_end];
        let points =
            attr_value(tag, "points").ok_or(SvgPathError::InvalidAttribute { name: "points" })?;
        parsed.push(parse_polyline(points)?);
        cursor = tag_end + 1;
    }

    match parsed.len() {
        0 => Err(SvgPathError::MissingPathLikeElement { path_index }),
        1 => Ok(parsed.remove(0)),
        _ => Err(SvgPathError::MultiplePathLikeElements { path_index }),
    }
}

fn parse_label(value: &str) -> Result<PathLabel, SvgPathError> {
    let mut path_index = None;
    let mut count = None;
    for part in value.split(',') {
        let Some((key, raw_value)) = part.split_once(':') else {
            return Err(invalid_label(value));
        };
        match key.trim() {
            "path" => path_index = Some(parse_u32(raw_value.trim())?),
            "count" => count = Some(parse_u32(raw_value.trim())?),
            _ => return Err(invalid_label(value)),
        }
    }
    let path_index = path_index.ok_or_else(|| invalid_label(value))?;
    let count = count.ok_or_else(|| invalid_label(value))?;
    if count == 0 {
        return Err(SvgPathError::ZeroCount { path_index });
    }
    Ok(PathLabel { path_index, count })
}

fn normalize_text_payload(raw: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;
    for c in raw.chars() {
        match c {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                output.push(' ');
            }
            _ if !inside_tag => output.push(c),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut search_start = 0usize;
    while let Some(found) = tag[search_start..].find(name) {
        let start = search_start + found;
        let after_name = start + name.len();
        if !is_attr_boundary(tag, start, after_name) {
            search_start = after_name;
            continue;
        }
        let rest = tag[after_name..].trim_start();
        let rest_offset = tag[after_name..].len() - rest.len();
        if !rest.starts_with('=') {
            search_start = after_name;
            continue;
        }
        let after_equals = after_name + rest_offset + 1;
        let value = tag[after_equals..].trim_start();
        let value_offset = tag[after_equals..].len() - value.len();
        let quote = value.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value_start = after_equals + value_offset + 1;
        let value_end = tag[value_start..]
            .find(quote)
            .map(|offset| value_start + offset)?;
        return Some(&tag[value_start..value_end]);
    }
    None
}

fn is_attr_boundary(tag: &str, start: usize, end: usize) -> bool {
    let before = tag[..start]
        .chars()
        .next_back()
        .is_none_or(|c| c.is_ascii_whitespace() || c == '<');
    let after = tag[end..]
        .chars()
        .next()
        .is_none_or(|c| c.is_ascii_whitespace() || c == '=');
    before && after
}

fn find_tag(svg: &str, name: &str, from: usize) -> Option<usize> {
    let needle = alloc::format!("<{name}");
    let mut cursor = from;
    while let Some(offset) = svg[cursor..].find(&needle) {
        let start = cursor + offset;
        let after_name = start + needle.len();
        if svg[after_name..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_whitespace() || c == '>' || c == '/')
        {
            return Some(start);
        }
        cursor = after_name;
    }
    None
}

fn parse_f32(value: &str) -> Result<f32, SvgPathError> {
    value.parse().map_err(|_| invalid_number(value))
}

fn parse_u32(value: &str) -> Result<u32, SvgPathError> {
    value.parse().map_err(|_| invalid_number(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::fixture::mapping::svg_path::SvgPathGeometry;

    #[test]
    fn parses_grouped_polyline_with_tspan_text() {
        let svg = r#"
<svg viewBox="0 0 10 10">
  <g data-role="outline"><path d="M0,0 L1,1"/></g>
  <g><polyline points="0 0 10 0 10 10"/><text><tspan>path:1,count:10</tspan></text></g>
</svg>
"#;
        let parsed = parse_svg_path_groups(svg).unwrap();
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].path_index, 1);
        assert_eq!(parsed.groups[0].count, 10);
        let SvgPathGeometry::Polyline(points) = &parsed.groups[0].geometry;
        assert_eq!(points.len(), 3);
    }

    #[test]
    fn rejects_ungrouped_mapping_text() {
        let svg = r#"<svg><text>path:1,count:10</text></svg>"#;
        assert!(matches!(
            parse_svg_path_groups(svg),
            Err(SvgPathError::UngroupedMappingText(_))
        ));
    }

    #[test]
    fn rejects_mapping_group_without_path() {
        let svg = r#"<svg><g><text>path:1,count:10</text></g></svg>"#;
        assert!(matches!(
            parse_svg_path_groups(svg),
            Err(SvgPathError::MissingPathLikeElement { path_index: 1 })
        ));
    }

    #[test]
    fn rejects_multiple_path_like_elements() {
        let svg = r#"
<svg><g>
  <path d="M0,0 L1,1"/>
  <polyline points="0 0 1 1"/>
  <text>path:1,count:10</text>
</g></svg>
"#;
        assert!(matches!(
            parse_svg_path_groups(svg),
            Err(SvgPathError::MultiplePathLikeElements { path_index: 1 })
        ));
    }

    #[test]
    fn rejects_extra_text_in_mapping_group() {
        let svg = r#"
<svg><g>
  <polyline points="0 0 1 1"/>
  <text>path:1,count:10</text>
  <text>ignored label</text>
</g></svg>
"#;
        assert!(matches!(
            parse_svg_path_groups(svg),
            Err(SvgPathError::MultipleTextElements)
        ));
    }

    #[test]
    fn rejects_malformed_labels() {
        let svg = r#"<svg><g><polyline points="0 0 1 1"/><text>path:one,count:10</text></g></svg>"#;
        assert!(matches!(
            parse_svg_path_groups(svg),
            Err(SvgPathError::InvalidNumber(_))
        ));
    }

    #[test]
    fn rejects_curve_commands() {
        let svg = r#"<svg><g><path d="M0,0 C1,1 2,2 3,3"/><text>path:1,count:10</text></g></svg>"#;
        assert!(matches!(
            parse_svg_path_groups(svg),
            Err(SvgPathError::UnsupportedCommand('C'))
        ));
    }
}
