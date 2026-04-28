//! Parse `// texture-spec:` and `// texture-data:` filetest directives.
//!
//! The main scan is driven from [`parse_test_file`](crate::parse::parse_test_file) in `mod.rs`.

use anyhow::Result;
use lps_shared::TextureBindingSpec;
use lps_shared::TextureFilter;
use lps_shared::TextureShapeHint;
use lps_shared::TextureStorageFormat;
use lps_shared::TextureWrap;

use super::test_type::TextureFixture;
use super::test_type::TextureFixtureChannel;
use super::test_type::TextureFixturePixel;
/// In-progress `// texture-data:` between the header and the end of the pixel block.
#[derive(Debug)]
pub struct TextureDataPartial {
    /// Sampler / binding name (matches `// texture-spec:` and the shader uniform).
    pub name: String,
    /// Texel width from the `WxH` header token.
    pub width: u32,
    /// Texel height from the `WxH` header token.
    pub height: u32,
    /// Declared storage format (must match `// texture-spec:` for the same name at validation time).
    pub format: TextureStorageFormat,
    /// Line of the `// texture-data:` header.
    pub header_line: usize,
    /// Accumulated pixels, row-major.
    pub pixels: Vec<TextureFixturePixel>,
}

/// Return value from [`process_texture_data_line`].
#[derive(Debug)]
pub enum TextureDataLineStep {
    /// Still inside the `// texture-data:` block; advance the file line index.
    Consuming,
    /// Block finished: insert `fixture` and optionally reprocess the closing line.
    BlockDone {
        /// Parsed `texture-data` block.
        fixture: TextureFixture,
        /// `true` if the line that ended the block is not a pixel row and must be parsed again.
        reprocess_current_line: bool,
    },
}

/// Returns `true` if `line` (logical, block fragments stripped) is a `// texture-spec:` line.
pub fn is_texture_spec_line(line: &str) -> bool {
    line.trim()
        .strip_prefix("//")
        .is_some_and(|r| r.trim_start().starts_with("texture-spec:"))
}

/// True when `line` is a `// texture-data:` header.
pub fn is_texture_data_header_line(line: &str) -> bool {
    line.trim()
        .strip_prefix("//")
        .is_some_and(|r| r.trim_start().starts_with("texture-data:"))
}

/// Parse `// texture-spec: <name> key=value...` (logical line, block fragments stripped).
pub fn parse_texture_spec_line(
    line: &str,
    line_number: usize,
) -> Result<(String, TextureBindingSpec)> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("//")
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: expected `// texture-spec:` line"))?
        .trim_start()
        .strip_prefix("texture-spec:")
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: expected `texture-spec:`"))?
        .trim();
    if rest.is_empty() {
        anyhow::bail!("line {line_number}: `texture-spec:` missing name and keys");
    }
    let mut words = rest.split_whitespace();
    let name = words
        .next()
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: `texture-spec:` missing name"))?
        .to_string();
    if name.is_empty() {
        anyhow::bail!("line {line_number}: `texture-spec:` empty name");
    }

    let mut format: Option<TextureStorageFormat> = None;
    let mut filter: Option<TextureFilter> = None;
    let mut shape_hint: Option<TextureShapeHint> = None;
    let mut wrap: Option<TextureWrap> = None;
    let mut wrap_x: Option<TextureWrap> = None;
    let mut wrap_y: Option<TextureWrap> = None;

    for pair in words {
        let (k, v) = pair.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "line {line_number}: expected `key=value` in texture-spec, got {pair:?}"
            )
        })?;
        let k = k.trim();
        let v = v.trim();
        if k.is_empty() || v.is_empty() {
            anyhow::bail!("line {line_number}: empty key or value in `key=value`");
        }
        match k {
            "format" => {
                format = Some(parse_format(v, line_number)?);
            }
            "filter" => {
                filter = Some(parse_filter(v, line_number)?);
            }
            "shape" => {
                shape_hint = Some(parse_shape(v, line_number)?);
            }
            "wrap" => {
                let w = parse_wrap_mode(v, line_number)?;
                wrap = Some(w);
            }
            "wrap_x" => {
                wrap_x = Some(parse_wrap_mode(v, line_number)?);
            }
            "wrap_y" => {
                wrap_y = Some(parse_wrap_mode(v, line_number)?);
            }
            _ => {
                anyhow::bail!("line {line_number}: unknown `texture-spec` key {k:?}");
            }
        }
    }

    let format = format
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: `texture-spec:` requires `format=`"))?;
    let filter = filter
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: `texture-spec:` requires `filter=`"))?;
    let shape_hint = shape_hint
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: `texture-spec:` requires `shape=`"))?;

    let (wx, wy) = match (wrap, wrap_x, wrap_y) {
        (_, Some(x), Some(y)) => (x, y),
        (Some(w), Some(x), None) => (x, w),
        (Some(w), None, Some(y)) => (w, y),
        (Some(w), None, None) => (w, w),
        (None, Some(_), None) => {
            anyhow::bail!("line {line_number}: `wrap_x` without `wrap_y` and no `wrap=` fallback");
        }
        (None, None, Some(_)) => {
            anyhow::bail!("line {line_number}: `wrap_y` without `wrap_x` and no `wrap=` fallback");
        }
        (None, None, None) => {
            anyhow::bail!(
                "line {line_number}: `texture-spec:` requires `wrap=` or both `wrap_x=` and `wrap_y=`"
            );
        }
    };

    let spec = TextureBindingSpec {
        format,
        filter,
        wrap_x: wx,
        wrap_y: wy,
        shape_hint,
    };
    Ok((name, spec))
}

/// Parse `// texture-data: <name> <W>x<H> <format>`.
pub fn parse_texture_data_header(line: &str, line_number: usize) -> Result<TextureDataPartial> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("//")
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: expected `// texture-data:`"))?
        .trim_start()
        .strip_prefix("texture-data:")
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: expected `texture-data:`"))?
        .trim();
    if rest.is_empty() {
        anyhow::bail!("line {line_number}: `texture-data:` missing body");
    }
    let mut words = rest.split_whitespace();
    let name = words
        .next()
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: `texture-data:` missing name"))?
        .to_string();
    let wh = words.next().ok_or_else(|| {
        anyhow::anyhow!("line {line_number}: `texture-data:` missing `WxH` after name")
    })?;
    let (w, h) = parse_width_height(wh, line_number)?;
    let format_word = words.next().ok_or_else(|| {
        anyhow::anyhow!("line {line_number}: `texture-data:` missing format after `WxH`")
    })?;
    let format = parse_format(format_word, line_number)?;
    if words.next().is_some() {
        anyhow::bail!("line {line_number}: `texture-data:` header: extra tokens after format");
    }
    Ok(TextureDataPartial {
        name,
        width: w,
        height: h,
        format,
        header_line: line_number,
        pixels: Vec::new(),
    })
}

/// Advance a `// texture-data:` in-progress state by one `logical` line.
pub fn process_texture_data_line(
    partial: &mut Option<TextureDataPartial>,
    logical: &str,
    line_number: usize,
) -> Result<TextureDataLineStep> {
    let Some(p) = partial.as_mut() else {
        anyhow::bail!("internal: missing texture-data partial at line {line_number}");
    };

    if logical.trim().is_empty() {
        return Ok(TextureDataLineStep::Consuming);
    }

    if line_is_blank_comment_separator(logical) {
        let f = into_texture_fixture(partial.take().unwrap())?;
        return Ok(TextureDataLineStep::BlockDone {
            fixture: f,
            reprocess_current_line: false,
        });
    }

    if is_harness_line_that_ends_texture_data(logical) {
        let f = into_texture_fixture(partial.take().unwrap())?;
        return Ok(TextureDataLineStep::BlockDone {
            fixture: f,
            reprocess_current_line: true,
        });
    }

    if !is_line_starts_with_line_comment(logical) {
        // GLSL: first non-`//` line ends the block; this line is shader source.
        let f = into_texture_fixture(partial.take().unwrap())?;
        return Ok(TextureDataLineStep::BlockDone {
            fixture: f,
            reprocess_current_line: true,
        });
    }

    let after_slash = comment_body_after_slashes(logical, line_number)?;
    if after_slash.is_empty() || after_slash.trim().is_empty() {
        let f = into_texture_fixture(partial.take().unwrap())?;
        return Ok(TextureDataLineStep::BlockDone {
            fixture: f,
            reprocess_current_line: false,
        });
    }

    for pixel_tok in after_slash.split_whitespace() {
        let px = parse_one_pixel(pixel_tok, p.format, line_number)?;
        p.pixels.push(px);
    }
    Ok(TextureDataLineStep::Consuming)
}

/// Build a [`TextureFixture`] and validate size vs `width*height` and channel count.
pub fn into_texture_fixture(partial: TextureDataPartial) -> Result<TextureFixture> {
    let expected = (partial.width as usize).saturating_mul(partial.height as usize);
    if partial.pixels.len() != expected {
        anyhow::bail!(
            "line {}: `texture-data` for {:?}: expected {expected} pixel(s) ({}×{}), got {}",
            partial.header_line,
            partial.name,
            partial.width,
            partial.height,
            partial.pixels.len()
        );
    }
    let ch = partial.format.channel_count();
    for (i, px) in partial.pixels.iter().enumerate() {
        if px.channels.len() != ch {
            anyhow::bail!(
                "line {}: pixel index {i}: expected {ch} channel(s) for format {:?}, got {}",
                partial.header_line,
                partial.format,
                px.channels.len()
            );
        }
    }
    let TextureDataPartial {
        name,
        width,
        height,
        format,
        header_line: line_number,
        pixels,
        ..
    } = partial;
    Ok(TextureFixture {
        name,
        width,
        height,
        format,
        pixels,
        line_number,
    })
}

/// Returns `true` for other filetest harness lines that are not `// texture-data:` pixel rows.
fn is_harness_line_that_ends_texture_data(line: &str) -> bool {
    if is_texture_spec_line(line) || is_texture_data_header_line(line) {
        return true;
    }
    if super::parse_test_type::parse_test_type(line).is_some() {
        return true;
    }
    if super::parse_target::parse_target_directive(line).is_some() {
        return true;
    }
    let t = line.trim();
    if line
        .trim()
        .strip_prefix("//")
        .map_or(false, |r| r.trim_start().starts_with("compile-opt"))
    {
        return true;
    }
    if t.trim_start().starts_with("// @") {
        return true;
    }
    if super::parse_set_uniform::parse_set_uniform_line(line).is_some() {
        return true;
    }
    if t.strip_prefix("//")
        .is_some_and(|r| r.trim_start().starts_with("EXPECT_SETUP_FAILURE:"))
    {
        return true;
    }
    if t.strip_prefix("//")
        .is_some_and(|r| r.trim_start().starts_with("expect-parse-failure:"))
    {
        return true;
    }
    if super::parse_run::parse_run_directive_line(line).is_some() {
        return true;
    }
    t.starts_with("// EXPECT_TRAP") || t.starts_with("// expected-error")
}

fn is_line_starts_with_line_comment(line: &str) -> bool {
    line.trim().starts_with("//")
}

fn line_is_blank_comment_separator(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("//") && t[2..].trim().is_empty()
}

fn comment_body_after_slashes(line: &str, line_number: usize) -> Result<&str> {
    let t = line.trim();
    if !t.starts_with("//") {
        anyhow::bail!("line {line_number}: expected `//` pixel row in texture-data block");
    }
    let rest = t[2..].trim();
    if rest.is_empty() {
        return Ok("");
    }
    Ok(rest)
}

fn parse_one_pixel(
    text: &str,
    format: TextureStorageFormat,
    line_number: usize,
) -> Result<TextureFixturePixel> {
    let expect = format.channel_count();
    let parts: Vec<&str> = text.split(',').filter(|p| !p.is_empty()).collect();
    if parts.len() != expect {
        anyhow::bail!(
            "line {line_number}: pixel {text:?} has {} channel part(s), expected {expect} for format {:?}",
            parts.len(),
            format
        );
    }
    let mut channels = Vec::with_capacity(expect);
    for ch in parts {
        channels.push(parse_channel(ch, line_number)?);
    }
    Ok(TextureFixturePixel { channels })
}

fn parse_channel(s: &str, line_number: usize) -> Result<TextureFixtureChannel> {
    let s = s.trim();
    if s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(TextureFixtureChannel::ExactHex(
            u16::from_str_radix(s, 16)
                .map_err(|e| anyhow::anyhow!("line {line_number}: bad hex {s:?}: {e}"))?,
        ));
    }
    if s == "." {
        anyhow::bail!("line {line_number}: invalid float {s:?}");
    }
    let f: f32 = s.parse().map_err(|e| {
        anyhow::anyhow!("line {line_number}: expected float or 4-hex u16, got {s:?}: {e}")
    })?;
    Ok(TextureFixtureChannel::NormalizedFloat(f))
}

fn parse_width_height(w_h: &str, line_number: usize) -> Result<(u32, u32)> {
    let (a, b) = w_h
        .split_once('x')
        .or_else(|| w_h.split_once('X'))
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: expected `WxH`, got {w_h:?}"))?;
    let w: u32 = a
        .parse()
        .map_err(|e| anyhow::anyhow!("line {line_number}: bad width: {e}"))?;
    let h: u32 = b
        .parse()
        .map_err(|e| anyhow::anyhow!("line {line_number}: bad height: {e}"))?;
    Ok((w, h))
}

fn parse_format(s: &str, line_number: usize) -> Result<TextureStorageFormat> {
    match s {
        "r16unorm" => Ok(TextureStorageFormat::R16Unorm),
        "rgb16unorm" => Ok(TextureStorageFormat::Rgb16Unorm),
        "rgba16unorm" => Ok(TextureStorageFormat::Rgba16Unorm),
        _ => anyhow::bail!(
            "line {line_number}: unknown texture format {s:?} (expected r16unorm, rgb16unorm, rgba16unorm)"
        ),
    }
}

fn parse_filter(s: &str, line_number: usize) -> Result<TextureFilter> {
    match s {
        "nearest" => Ok(TextureFilter::Nearest),
        "linear" => Ok(TextureFilter::Linear),
        _ => anyhow::bail!("line {line_number}: unknown filter {s:?} (expected nearest, linear)"),
    }
}

fn parse_shape(s: &str, line_number: usize) -> Result<TextureShapeHint> {
    match s {
        "2d" => Ok(TextureShapeHint::General2D),
        "height-one" | "height_one" => Ok(TextureShapeHint::HeightOne),
        _ => anyhow::bail!(
            "line {line_number}: unknown shape {s:?} (expected 2d, height-one, height_one)"
        ),
    }
}

fn parse_wrap_mode(s: &str, line_number: usize) -> Result<TextureWrap> {
    match s {
        "clamp" | "clamp-to-edge" => Ok(TextureWrap::ClampToEdge),
        "repeat" => Ok(TextureWrap::Repeat),
        "mirror-repeat" | "mirror_repeat" => Ok(TextureWrap::MirrorRepeat),
        _ => anyhow::bail!("line {line_number}: unknown wrap mode {s:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_test_file;
    use lps_shared::TextureShapeHint;
    use lps_shared::TextureStorageFormat;
    use lps_shared::TextureWrap;

    fn tmp_glsl(suffix: &str, content: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("lps_ft_tex_{}_{}.glsl", suffix, std::process::id()));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn minimal_one_spec_and_one_fixture() {
        let p = tmp_glsl(
            "minimal",
            r"// test run
// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0
float f() { return 1.0; }
// run: f() ~= 1.0
",
        );
        let tf = parse_test_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(tf.texture_specs.len(), 1);
        assert_eq!(tf.texture_fixtures.len(), 1);
        let fx = tf.texture_fixtures.get("inputColor").expect("fixture");
        assert_eq!(fx.width, 2);
        assert_eq!(fx.height, 1);
        assert_eq!(fx.format, TextureStorageFormat::Rgba16Unorm);
        assert_eq!(fx.pixels.len(), 2);
    }

    #[test]
    fn wrap_clamp_both_axes() {
        let (_, s) = parse_texture_spec_line(
            "// texture-spec: u format=rgba16unorm filter=nearest wrap=clamp shape=2d",
            1,
        )
        .unwrap();
        assert_eq!(s.wrap_x, TextureWrap::ClampToEdge);
        assert_eq!(s.wrap_y, TextureWrap::ClampToEdge);
    }

    #[test]
    fn wrap_x_wrap_y() {
        let (_, s) = parse_texture_spec_line(
            "// texture-spec: u format=rgba16unorm filter=nearest wrap_x=repeat wrap_y=clamp shape=2d",
            1,
        )
        .unwrap();
        assert_eq!(s.wrap_x, TextureWrap::Repeat);
        assert_eq!(s.wrap_y, TextureWrap::ClampToEdge);
    }

    #[test]
    fn wrap_axis_can_override_wrap_fallback() {
        let (_, s) = parse_texture_spec_line(
            "// texture-spec: u format=rgba16unorm filter=nearest wrap=clamp wrap_x=repeat shape=2d",
            1,
        )
        .unwrap();
        assert_eq!(s.wrap_x, TextureWrap::Repeat);
        assert_eq!(s.wrap_y, TextureWrap::ClampToEdge);
    }

    #[test]
    fn shape_height_one_spellings() {
        for sh in ["height-one", "height_one"] {
            let (_, s) = parse_texture_spec_line(
                &format!("// texture-spec: u format=r16unorm filter=nearest wrap=clamp shape={sh}"),
                1,
            )
            .unwrap();
            assert_eq!(s.shape_hint, TextureShapeHint::HeightOne);
        }
    }

    #[test]
    fn reject_duplicate_spec_name() {
        let p = tmp_glsl(
            "dup_spec",
            r"// test run
// texture-spec: t format=r16unorm filter=nearest wrap=clamp shape=2d
// texture-spec: t format=r16unorm filter=nearest wrap=clamp shape=2d
float f() { return 1.0; }
// run: f() ~= 1.0
",
        );
        let r = parse_test_file(&p);
        let _ = std::fs::remove_file(&p);
        assert!(r.is_err());
        let msg = r.err().unwrap().to_string();
        assert!(msg.contains("duplicate `texture-spec`"), "{msg}");
    }

    #[test]
    fn reject_duplicate_fixture_name() {
        let p = tmp_glsl(
            "dup_fx",
            r"// test run
// texture-spec: t format=r16unorm filter=nearest wrap=clamp shape=2d
// texture-data: t 1x1 r16unorm
// 1.0
// texture-data: t 1x1 r16unorm
// 1.0
float f() { return 1.0; }
// run: f() ~= 1.0
",
        );
        let r = parse_test_file(&p);
        let _ = std::fs::remove_file(&p);
        assert!(r.is_err());
    }

    #[test]
    fn reject_bad_filter() {
        let e = parse_texture_spec_line(
            "// texture-spec: u format=rgba16unorm filter=nope wrap=clamp shape=2d",
            1,
        );
        assert!(e.is_err());
    }

    #[test]
    fn reject_bad_shape() {
        let e = parse_texture_spec_line(
            "// texture-spec: u format=rgba16unorm filter=nearest wrap=clamp shape=3d",
            1,
        );
        assert!(e.is_err());
    }

    #[test]
    fn reject_bad_wrap() {
        let e = parse_texture_spec_line(
            "// texture-spec: u format=rgba16unorm filter=nearest wrap=weird shape=2d",
            1,
        );
        assert!(e.is_err());
    }

    #[test]
    fn reject_malformed_texture_data_header() {
        let e = parse_texture_data_header("// texture-data: a 1x1", 1);
        assert!(e.is_err());
    }

    #[test]
    fn ignore_texture_directives_inside_block_comment() {
        let p = tmp_glsl(
            "block_cmt",
            r"// test run
// texture-spec: t format=r16unorm filter=nearest wrap=clamp shape=2d
// texture-data: t 1x1 r16unorm
// 0.5
/*
// texture-spec: t2 format=r16unorm filter=nearest wrap=clamp shape=2d
*/
float f() { return 1.0; }
// run: f() ~= 1.0
",
        );
        let tf = parse_test_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(tf.texture_specs.len(), 1);
        assert_eq!(tf.texture_fixtures.len(), 1);
        assert!(tf.texture_specs.contains_key("t"));
    }

    /// Pixel rows use 4-hex for exact unorm16 storage.
    #[test]
    fn exact_hex_rgba() {
        let p = tmp_glsl(
            "hex",
            r"// test run
// texture-spec: t format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: t 1x1 rgba16unorm
// FFFF,0000,8000,FFFF
float f() { return 0.0; }
// run: f() ~= 0.0
",
        );
        let tf = parse_test_file(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        let ch = &tf.texture_fixtures.get("t").unwrap().pixels[0].channels;
        assert!(matches!(&ch[0], TextureFixtureChannel::ExactHex(0xFFFF)));
    }
}
