//! The reference background image: editor-side tracing state, never part of
//! the map2d document (the doc has a 10KB asset budget; reference art is
//! routinely 20KB+). The page host owns loading and persistence — the
//! editor just renders it under the authored geometry.

/// A traceable background image, doc-space, anchored at the origin.
///
/// `size` is explicit doc units rather than the browser's intrinsic sizing
/// because the common case — an Illustrator SVG with a `viewBox` and no
/// `width`/`height` attributes — has no usable intrinsic size at all (an
/// SVG `<image>` would fall back to 300×150). Hosts fill it from the
/// viewBox (SVG) or the decoded pixel size (raster), so a sketch whose
/// viewBox IS the doc canvas lands at exactly the authored coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceImage {
    pub data_url: String,
    /// 0..=1 render opacity.
    pub opacity: f32,
    /// Rendered doc-space size `[width, height]`.
    pub size: [f32; 2],
}

/// Default opacity for a freshly loaded reference: visible enough to trace,
/// dim enough that authored lamps read on top.
pub const DEFAULT_REFERENCE_OPACITY: f32 = 0.5;

impl ReferenceImage {
    /// Serialize for the host's persistence slot.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "data_url": self.data_url,
            "opacity": self.opacity,
            "size": self.size,
        })
        .to_string()
    }

    /// Parse a persisted reference; `None` on anything unreadable (the host
    /// treats that as "no reference" — tracing state is not worth a refusal
    /// flow).
    #[must_use]
    pub fn from_json(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let data_url = value.get("data_url")?.as_str()?.to_string();
        let opacity = value.get("opacity")?.as_f64()? as f32;
        let size = value.get("size")?.as_array()?;
        let width = size.first()?.as_f64()? as f32;
        let height = size.get(1)?.as_f64()? as f32;
        if !(width > 0.0 && height > 0.0) {
            return None;
        }
        Some(Self {
            data_url,
            opacity: opacity.clamp(0.0, 1.0),
            size: [width, height],
        })
    }
}

/// The doc-space size an SVG reference renders at: its `width`/`height`
/// attributes when present, else the `viewBox` extent (the Illustrator
/// export shape — viewBox only). `None` means the text isn't an SVG we can
/// size, and the host should refuse it rather than guess.
#[must_use]
pub fn svg_reference_size(svg_text: &str) -> Option<[f32; 2]> {
    let open_tag = {
        let start = svg_text.find("<svg")?;
        let end = svg_text[start..].find('>')? + start;
        &svg_text[start..=end]
    };
    if let (Some(width), Some(height)) = (attr_f32(open_tag, "width"), attr_f32(open_tag, "height"))
    {
        return Some([width, height]);
    }
    let view_box = attr_value(open_tag, "viewBox")?;
    let mut parts = view_box.split_whitespace().flat_map(|p| p.split(','));
    let _min_x = parts.next()?;
    let _min_y = parts.next()?;
    let width: f32 = parts.next()?.trim().parse().ok()?;
    let height: f32 = parts.next()?.trim().parse().ok()?;
    (width > 0.0 && height > 0.0).then_some([width, height])
}

fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    // Attribute scan on the raw open tag: enough for real-world SVG exports,
    // and a miss just refuses the file.
    let pattern = format!("{name}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn attr_f32(tag: &str, name: &str) -> Option<f32> {
    // Illustrator writes plain numbers or "px" units; anything else (%, em)
    // is not doc-space and falls through to the viewBox.
    let raw = attr_value(tag, name)?;
    raw.trim().trim_end_matches("px").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        let reference = ReferenceImage {
            data_url: "data:image/svg+xml;base64,AAAA".into(),
            opacity: 0.35,
            size: [1163.3, 1165.8],
        };
        let parsed = ReferenceImage::from_json(&reference.to_json()).expect("parses");
        assert_eq!(parsed, reference);
    }

    #[test]
    fn unreadable_persisted_state_is_none_not_a_crash() {
        assert_eq!(ReferenceImage::from_json("{not json"), None);
        assert_eq!(ReferenceImage::from_json("{}"), None);
        assert_eq!(
            ReferenceImage::from_json(r#"{"data_url":"d","opacity":0.5,"size":[0,10]}"#),
            None,
            "zero-size reference is refused"
        );
    }

    /// The exact shape that motivated explicit sizing: Illustrator exports
    /// a viewBox and no width/height, so intrinsic sizing would be 300x150.
    #[test]
    fn illustrator_viewbox_only_svg_sizes_from_the_viewbox() {
        let svg = r#"<?xml version="1.0"?>
<svg id="Layer_1" xmlns="http://www.w3.org/2000/svg" version="1.1" viewBox="0 0 1163.3 1165.8">
  <line x1="0" y1="0" x2="1" y2="1"/>
</svg>"#;
        assert_eq!(svg_reference_size(svg), Some([1163.3, 1165.8]));
    }

    #[test]
    fn explicit_width_height_win_over_the_viewbox() {
        let svg = r#"<svg width="640px" height="480" viewBox="0 0 10 10"></svg>"#;
        assert_eq!(svg_reference_size(svg), Some([640.0, 480.0]));
    }

    #[test]
    fn comma_separated_viewbox_parses() {
        let svg = r#"<svg viewBox="0,0,320,200"></svg>"#;
        assert_eq!(svg_reference_size(svg), Some([320.0, 200.0]));
    }

    #[test]
    fn non_svg_text_is_refused() {
        assert_eq!(svg_reference_size("PNG binary soup"), None);
        assert_eq!(svg_reference_size("<svg viewBox=\"0 0\">"), None);
    }
}
