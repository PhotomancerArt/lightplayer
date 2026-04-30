//! TOML and CSS helpers for color, palette, and gradient [`WireValue`] shapes.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::WireValue;
use lpc_model::kind::{Kind, MAX_GRADIENT_STOPS, MAX_PALETTE_LEN};

use super::toml_parse::{
    FromTomlError, find_field_array, find_field_f32, find_field_i32, find_field_vec3_value,
    slice_to_vec3_toml, toml_f32, vec3_from_toml, vec3_to_toml_array,
};

/// Map authoring string → `I32` tag for `Color.*.space` / gradient method / similar.
pub(super) fn colorspace_id(s: &str) -> Result<i32, FromTomlError> {
    let s = s.to_lowercase();
    let id = match s.as_str() {
        "oklch" => 0,
        "oklab" => 1,
        "linear_srgb" | "linearrgb" => 2,
        "srgb" => 3,
        "hsl" => 4,
        "hsv" => 5,
        _ => {
            return Err(FromTomlError(format!("unknown color space `{s}`")));
        }
    };
    Ok(id)
}

/// Inverse of [`colorspace_id`] for TOML output (snake_case strings, `docs/design/color.md` §4).
pub(super) fn colorspace_name(id: i32) -> Result<&'static str, FromTomlError> {
    match id {
        0 => Ok("oklch"),
        1 => Ok("oklab"),
        2 => Ok("linear_srgb"),
        3 => Ok("srgb"),
        4 => Ok("hsl"),
        5 => Ok("hsv"),
        _ => Err(FromTomlError::msg("unknown color space I32 id")),
    }
}

/// Function name for CSS-style `Color` TOML serialization: `name(a b c)`.
pub(super) fn colorspace_css_serialize_name(id: i32) -> Result<&'static str, FromTomlError> {
    match id {
        0 => Ok("oklch"),
        1 => Ok("oklab"),
        2 => Ok("linear_srgb"),
        3 => Ok("srgb"),
        4 => Ok("hsl"),
        5 => Ok("hsv"),
        _ => Err(FromTomlError::msg("unknown color space I32 id")),
    }
}

fn split_css_arg_tokens(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for part in body.split(',') {
        for tok in part.split_whitespace() {
            if !tok.is_empty() {
                out.push(tok);
            }
        }
    }
    out
}

fn parse_f32_loose(s: &str) -> Result<f32, FromTomlError> {
    s.parse::<f32>()
        .map_err(|_| FromTomlError(format!("color: invalid number `{s}`")))
}

/// CSS `100%` style token → 0.0..=1.0
fn parse_css_percent(s: &str) -> Result<f32, FromTomlError> {
    let Some(stripped) = s.strip_suffix('%') else {
        return Err(FromTomlError::msg("color: internal percent parse"));
    };
    let p = parse_f32_loose(stripped.trim())?;
    Ok(p / 100.0)
}

/// Hue: `120`, `120deg` (and optional `turn`/`rad` as multiples of 360/2π to degrees).
fn parse_css_hue(s: &str) -> Result<f32, FromTomlError> {
    let t = s.trim();
    if let Some(n) = t.strip_suffix("deg") {
        return parse_f32_loose(n.trim());
    }
    if let Some(n) = t.strip_suffix("turn") {
        return Ok(parse_f32_loose(n.trim())? * 360.0);
    }
    if let Some(n) = t.strip_suffix("rad") {
        return Ok(parse_f32_loose(n.trim())? * 180.0 / core::f32::consts::PI);
    }
    if let Some(n) = t.strip_suffix("grad") {
        return Ok(parse_f32_loose(n.trim())? * 360.0 / 400.0);
    }
    parse_f32_loose(t)
}

/// sRGB 0–1 from one `rgb()` / `r` channel: `%` is CSS semantics; otherwise >1 means 0–255.
fn parse_rgb_channel(tok: &str) -> Result<f32, FromTomlError> {
    if tok.ends_with('%') {
        return parse_css_percent(tok);
    }
    let v = parse_f32_loose(tok)?;
    if v > 1.0 {
        return Ok((v / 255.0).clamp(0.0, 1.0));
    }
    Ok(v.clamp(0.0, 1.0))
}

/// `hsl` / `hsv` S and L/V: `%` → 0–1; else plain number, or 0–100 when > 1.
fn parse_hsl_hsv_sl(tok: &str) -> Result<f32, FromTomlError> {
    if tok.ends_with('%') {
        return parse_css_percent(tok);
    }
    let v = parse_f32_loose(tok)?;
    if v > 1.0 { Ok(v / 100.0) } else { Ok(v) }
}

fn parse_hex_color(s: &str) -> Result<(i32, [f32; 3]), FromTomlError> {
    let s = s.trim();
    if !s.starts_with('#') {
        return Err(FromTomlError::msg("color: internal hex parse"));
    }
    let hex = s.strip_prefix('#').unwrap();
    let (r, g, b) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16);
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16);
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16);
            (r, g, b)
        }
        6 | 8 => {
            let h = if hex.len() == 8 { &hex[..6] } else { hex };
            let r = u8::from_str_radix(&h[0..2], 16);
            let g = u8::from_str_radix(&h[2..4], 16);
            let b = u8::from_str_radix(&h[4..6], 16);
            (r, g, b)
        }
        _ => {
            return Err(FromTomlError::msg(
                "color: hex must be #rgb, #rrggbb, or #rrggbbaa",
            ));
        }
    };
    let r = r.map_err(|_| FromTomlError::msg("color: bad hex (red)"))?;
    let g = g.map_err(|_| FromTomlError::msg("color: bad hex (green)"))?;
    let b = b.map_err(|_| FromTomlError::msg("color: bad hex (blue)"))?;
    let rf = f32::from(r) / 255.0;
    let gf = f32::from(g) / 255.0;
    let bf = f32::from(b) / 255.0;
    Ok((3, [rf, gf, bf]))
}

fn color_struct_from_space_coords(space: i32, c: [f32; 3]) -> WireValue {
    WireValue::Struct {
        name: Some(String::from("Color")),
        fields: vec![
            (String::from("space"), WireValue::I32(space)),
            (String::from("coords"), WireValue::Vec3(c)),
        ],
    }
}

/// Parse a CSS-style color string to `(space_id, coords)` for `Color` literals.
fn parse_css_color_string(s: &str) -> Result<(i32, [f32; 3]), FromTomlError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(FromTomlError::msg("color: empty CSS string"));
    }
    if s.starts_with('#') {
        return parse_hex_color(s);
    }
    let open = s
        .find('(')
        .ok_or_else(|| FromTomlError::msg("color: CSS function needs `(`"))?;
    let close = s
        .rfind(')')
        .ok_or_else(|| FromTomlError::msg("color: CSS function needs `)`"))?;
    if close <= open {
        return Err(FromTomlError::msg("color: invalid `()` in CSS color"));
    }
    if close != s.len() - 1 {
        return Err(FromTomlError::msg(
            "color: unexpected text after `)` in CSS color",
        ));
    }
    let fname = s[..open].trim().to_lowercase();
    let inner = s[open + 1..close].trim();
    let toks = split_css_arg_tokens(inner);
    if toks.len() != 3 {
        return Err(FromTomlError(format!(
            "color: expected 3 channel values, got {} in `{fname}(…)`",
            toks.len()
        )));
    }
    let t0 = toks[0];
    let t1 = toks[1];
    let t2 = toks[2];
    let out = match fname.as_str() {
        "oklch" | "oklab" => {
            let x = parse_f32_loose(t0)?;
            let y = parse_f32_loose(t1)?;
            let z = parse_f32_loose(t2)?;
            let sp = if fname == "oklch" { 0 } else { 1 };
            (sp, [x, y, z])
        }
        "linear_srgb" | "linearrgb" => {
            let x = parse_f32_loose(t0)?;
            let y = parse_f32_loose(t1)?;
            let z = parse_f32_loose(t2)?;
            (2, [x, y, z])
        }
        "srgb" => {
            let x = parse_rgb_channel(t0)?;
            let y = parse_rgb_channel(t1)?;
            let z = parse_rgb_channel(t2)?;
            (3, [x, y, z])
        }
        "rgb" | "rgba" => {
            let x = parse_rgb_channel(t0)?;
            let y = parse_rgb_channel(t1)?;
            let z = parse_rgb_channel(t2)?;
            (3, [x, y, z])
        }
        "hsl" | "hsla" => {
            let h = parse_css_hue(t0)?;
            let s = parse_hsl_hsv_sl(t1)?;
            let l = parse_hsl_hsv_sl(t2)?;
            (4, [h, s, l])
        }
        "hsv" | "hsva" => {
            let h = parse_css_hue(t0)?;
            let s = parse_hsl_hsv_sl(t1)?;
            let v = parse_hsl_hsv_sl(t2)?;
            (5, [h, s, v])
        }
        _ => {
            return Err(FromTomlError(format!(
                "color: unknown CSS color function `{fname}`"
            )));
        }
    };
    Ok(out)
}

/// Trim trailing zeros for a compact TOML/CSS representation.
pub(super) fn fmt_css_coord(f: f32) -> String {
    let f = if f == 0.0f32 { 0.0f32 } else { f };
    let mut s = format!("{f:.6}");
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    s
}

fn interp_method_id(s: &str) -> Result<i32, FromTomlError> {
    let s = s.to_lowercase();
    match s.as_str() {
        "linear" => Ok(0),
        "cubic" => Ok(1),
        "step" => Ok(2),
        _ => Err(FromTomlError(format!(
            "unknown gradient interpolation method `{s}`"
        ))),
    }
}

fn interp_method_name(id: i32) -> Result<&'static str, FromTomlError> {
    match id {
        0 => Ok("linear"),
        1 => Ok("cubic"),
        2 => Ok("step"),
        _ => Err(FromTomlError::msg("unknown gradient method I32 id")),
    }
}

pub(super) fn wire_value_color(v: &toml::Value) -> Result<WireValue, FromTomlError> {
    match v {
        toml::Value::String(s) => {
            let (id, c) = parse_css_color_string(s)?;
            Ok(color_struct_from_space_coords(id, c))
        }
        toml::Value::Table(t) => wire_value_color_table(t),
        _ => Err(FromTomlError::msg(
            "color: expected a CSS string or a table { space, coords }",
        )),
    }
}

fn wire_value_color_table(
    t: &toml::map::Map<String, toml::Value>,
) -> Result<WireValue, FromTomlError> {
    let space = t
        .get("space")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FromTomlError::msg("color: missing `space` (string)"))?;
    let coords = t
        .get("coords")
        .ok_or_else(|| FromTomlError::msg("color: missing `coords`"))?;
    let v3 = vec3_from_toml(coords, "color.coords")?;
    Ok(WireValue::Struct {
        name: Some(String::from("Color")),
        fields: vec![
            (String::from("space"), WireValue::I32(colorspace_id(space)?)),
            (String::from("coords"), v3),
        ],
    })
}

pub(super) fn wire_value_color_palette(
    t: &toml::map::Map<String, toml::Value>,
) -> Result<WireValue, FromTomlError> {
    let space = t
        .get("space")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FromTomlError::msg("color_palette: missing `space`"))?;
    let entries = t
        .get("entries")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| FromTomlError::msg("color_palette: missing `entries` array"))?;
    let count = if let Some(c) = t.get("count").and_then(toml::Value::as_integer) {
        c as u32
    } else {
        entries.len() as u32
    };
    if count as usize > MAX_PALETTE_LEN as usize {
        return Err(FromTomlError::msg(
            "color_palette: count exceeds MAX_PALETTE_LEN",
        ));
    }
    if (entries.len() as u32) < count {
        return Err(FromTomlError::msg(
            "color_palette: not enough `entries` for `count`",
        ));
    }
    let mut v3s: Vec<WireValue> = Vec::new();
    for e in entries.iter().take(count as usize) {
        v3s.push(vec3_from_toml(e, "color_palette.entries")?);
    }
    while v3s.len() < MAX_PALETTE_LEN as usize {
        v3s.push(WireValue::Vec3([0.0, 0.0, 0.0]));
    }
    let entries_lps = WireValue::Array(v3s);
    Ok(WireValue::Struct {
        name: Some(String::from("ColorPalette")),
        fields: vec![
            (String::from("space"), WireValue::I32(colorspace_id(space)?)),
            (String::from("count"), WireValue::I32(count as i32)),
            (String::from("entries"), entries_lps),
        ],
    })
}

pub(super) fn wire_value_gradient(
    t: &toml::map::Map<String, toml::Value>,
) -> Result<WireValue, FromTomlError> {
    let space = t
        .get("space")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FromTomlError::msg("gradient: missing `space`"))?;
    let method = t
        .get("method")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FromTomlError::msg("gradient: missing `method`"))?;
    let method_id = interp_method_id(method)?;
    let stops = t
        .get("stops")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| FromTomlError::msg("gradient: missing `stops` array"))?;
    let count = if let Some(c) = t.get("count").and_then(toml::Value::as_integer) {
        c as u32
    } else {
        stops.len() as u32
    };
    if count as usize > MAX_GRADIENT_STOPS as usize {
        return Err(FromTomlError::msg(
            "gradient: count exceeds MAX_GRADIENT_STOPS",
        ));
    }
    if (stops.len() as u32) < count {
        return Err(FromTomlError::msg(
            "gradient: not enough `stops` for `count`",
        ));
    }
    let mut out: Vec<WireValue> = Vec::new();
    for s in stops.iter().take(count as usize) {
        out.push(gradient_stop_from_toml(s)?);
    }
    while out.len() < MAX_GRADIENT_STOPS as usize {
        out.push(gradient_stop_default());
    }
    let stops_lps = WireValue::Array(out);
    Ok(WireValue::Struct {
        name: Some(String::from("Gradient")),
        fields: vec![
            (String::from("space"), WireValue::I32(colorspace_id(space)?)),
            (String::from("method"), WireValue::I32(method_id)),
            (String::from("count"), WireValue::I32(count as i32)),
            (String::from("stops"), stops_lps),
        ],
    })
}

fn gradient_stop_from_toml(v: &toml::Value) -> Result<WireValue, FromTomlError> {
    let t = v
        .as_table()
        .ok_or_else(|| FromTomlError::msg("gradient stop must be a table"))?;
    let at = t
        .get("at")
        .ok_or_else(|| FromTomlError::msg("gradient stop: missing `at`"))?;
    let c = t
        .get("c")
        .ok_or_else(|| FromTomlError::msg("gradient stop: missing `c` (vec3)"))?;
    let cv = vec3_from_toml(c, "stop.c")?;
    Ok(WireValue::Struct {
        name: Some(String::from("GradientStop")),
        fields: vec![
            (String::from("at"), WireValue::F32(toml_f32(at)?)),
            (String::from("c"), cv),
        ],
    })
}

fn gradient_stop_default() -> WireValue {
    WireValue::Struct {
        name: Some(String::from("GradientStop")),
        fields: vec![
            (String::from("at"), WireValue::F32(0.0)),
            (String::from("c"), WireValue::Vec3([0.0, 0.0, 0.0])),
        ],
    }
}

/// Parse `WireValue` for struct kinds (Color, ColorPalette, Gradient).
pub(super) fn from_toml_struct_kind(
    value: &toml::Value,
    k: Kind,
) -> Result<WireValue, FromTomlError> {
    match k {
        Kind::Color => wire_value_color(value),
        Kind::ColorPalette => {
            let t = value
                .as_table()
                .ok_or_else(|| FromTomlError::msg("expected a TOML table"))?;
            wire_value_color_palette(t)
        }
        Kind::Gradient => {
            let t = value
                .as_table()
                .ok_or_else(|| FromTomlError::msg("expected a TOML table"))?;
            wire_value_gradient(t)
        }
        _ => Err(FromTomlError::msg("internal: not a struct color kind")),
    }
}

pub(super) fn wire_color_to_toml(v: &WireValue) -> Result<toml::Value, FromTomlError> {
    let WireValue::Struct { name, fields } = v else {
        return Err(FromTomlError::msg(
            "Color literal must be a struct WireValue",
        ));
    };
    if name.as_deref() != Some("Color") {
        return Err(FromTomlError::msg("Color literal: wrong struct name"));
    }
    let sp = find_field_i32(fields, "space")?;
    let co = find_field_vec3_value(fields, "coords")?;
    let css = colorspace_css_serialize_name(sp)?;
    let s = format!(
        "{}({} {} {})",
        css,
        fmt_css_coord(co[0]),
        fmt_css_coord(co[1]),
        fmt_css_coord(co[2])
    );
    Ok(toml::Value::String(s))
}

pub(super) fn wire_color_palette_to_toml(v: &WireValue) -> Result<toml::Value, FromTomlError> {
    let WireValue::Struct { name, fields } = v else {
        return Err(FromTomlError::msg(
            "ColorPalette must be a struct WireValue",
        ));
    };
    if name.as_deref() != Some("ColorPalette") {
        return Err(FromTomlError::msg("ColorPalette: wrong struct name"));
    }
    let sp = find_field_i32(fields, "space")?;
    let count = find_field_i32(fields, "count")? as u32;
    let entries = find_field_array(fields, "entries")?;
    let mut m: toml::map::Map<String, toml::Value> = toml::map::Map::new();
    m.insert(
        String::from("space"),
        toml::Value::String(colorspace_name(sp)?.to_string()),
    );
    m.insert(
        "count".to_string(),
        toml::Value::Integer(i64::from(count as i32)),
    );
    let arr = slice_to_vec3_toml(&entries[0..(count as usize).min(MAX_PALETTE_LEN as usize)])?;
    m.insert("entries".to_string(), toml::Value::Array(arr));
    Ok(toml::Value::Table(m))
}

pub(super) fn wire_gradient_to_toml(v: &WireValue) -> Result<toml::Value, FromTomlError> {
    let WireValue::Struct { name, fields } = v else {
        return Err(FromTomlError::msg("Gradient must be a struct WireValue"));
    };
    if name.as_deref() != Some("Gradient") {
        return Err(FromTomlError::msg("Gradient: wrong struct name"));
    }
    let sp = find_field_i32(fields, "space")?;
    let method = find_field_i32(fields, "method")?;
    let count = find_field_i32(fields, "count")? as u32;
    let stops = find_field_array(fields, "stops")?;
    let mut m: toml::map::Map<String, toml::Value> = toml::map::Map::new();
    m.insert(
        String::from("space"),
        toml::Value::String(colorspace_name(sp)?.to_string()),
    );
    m.insert(
        "method".to_string(),
        toml::Value::String(interp_method_name(method)?.to_string()),
    );
    m.insert(
        "count".to_string(),
        toml::Value::Integer(i64::from(count as i32)),
    );
    let n = (count as usize)
        .min(stops.len())
        .min(MAX_GRADIENT_STOPS as usize);
    let mut a = Vec::new();
    for s in &stops[..n] {
        a.push(gradient_stop_to_toml(s)?);
    }
    m.insert("stops".to_string(), toml::Value::Array(a));
    Ok(toml::Value::Table(m))
}

fn gradient_stop_to_toml(s: &WireValue) -> Result<toml::Value, FromTomlError> {
    let WireValue::Struct { fields, name } = s else {
        return Err(FromTomlError::msg("stop must be struct"));
    };
    if name.as_deref() != Some("GradientStop") {
        return Err(FromTomlError::msg("stop: bad name"));
    }
    let at = find_field_f32(fields, "at")?;
    let c = find_field_vec3_value(fields, "c")?;
    let mut t: toml::map::Map<String, toml::Value> = toml::map::Map::new();
    t.insert("at".to_string(), toml::Value::Float(f64::from(at)));
    t.insert("c".to_string(), vec3_to_toml_array(&c)?);
    Ok(toml::Value::Table(t))
}
