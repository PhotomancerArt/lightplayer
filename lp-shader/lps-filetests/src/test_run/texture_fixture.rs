//! Validate and encode parsed [`crate::parse::test_type::TextureFixture`] rows into
//! tightly packed little-endian unorm16 texture storage bytes.
//!
//! Float channels use the same encoding as
//! `lp-shader` render-texture / pixel-shader test helpers: scale by `65536`, round
//! to Q32, then map to unorm16 storage (see `lp-shader::tests::unorm16_bytes_from_f32`).

use crate::parse::test_type::{
    TextureFixture, TextureFixtureChannel, TextureFixtures, TextureSpecs,
};
use crate::test_run::filetest_lpvm::{CompiledShader, FiletestInstance};
use lps_shared::{LpsTexture2DDescriptor, TextureShapeHint, TextureStorageFormat};
use lpvm::{LpsValueF32, LpvmBuffer};

/// Tightly packed host-side texture bytes for a single fixture (see [`encode_texture_fixture`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTextureFixture {
    /// Sampler / binding name (from the `// texture-data:` block).
    pub name: String,
    /// Texel width.
    pub width: u32,
    /// Texel height.
    pub height: u32,
    /// Storage format (must match the fixture header).
    pub format: TextureStorageFormat,
    /// Row-major, tightly packed unorm16 channel bytes.
    pub bytes: Vec<u8>,
    /// Bytes per row (M2: `width * format.bytes_per_pixel()`).
    pub row_stride: u32,
}

/// Ensure every `// texture-spec:` has a matching `// texture-data:` block (and vice versa),
/// formats agree, and `shape=height-one` fixtures are 1 texel tall.
pub fn validate_runtime_texture_fixtures(
    specs: &TextureSpecs,
    fixtures: &TextureFixtures,
) -> anyhow::Result<()> {
    if specs.is_empty() && fixtures.is_empty() {
        return Ok(());
    }

    for (name, spec) in specs {
        let Some(fixture) = fixtures.get(name) else {
            anyhow::bail!(
                "texture uniform {name:?}: no runtime fixture (missing // texture-data: for this name)"
            );
        };
        if fixture.format != spec.format {
            anyhow::bail!(
                "texture fixture {name:?}: format {:?} does not match // texture-spec: format {:?}",
                fixture.format,
                spec.format
            );
        }
        if spec.shape_hint == TextureShapeHint::HeightOne && fixture.height != 1 {
            anyhow::bail!(
                "texture fixture {name:?}: height {} does not match texture-spec shape=height-one (expected 1)",
                fixture.height
            );
        }
    }

    for name in fixtures.keys() {
        if !specs.contains_key(name) {
            anyhow::bail!(
                "texture fixture {name:?}: no matching // texture-spec: (extra // texture-data:)"
            );
        }
    }

    Ok(())
}

/// Per `// run:`: validate fixtures, allocate shared memory, write encoded texels, bind
/// [`LpsValueF32::Texture2D`] for each spec before ordinary `// set_uniform:` lines.
///
/// Returns allocation handles; keep them alive until the run finishes.
pub fn bind_texture_fixtures_for_run(
    compiled: &CompiledShader,
    inst: &mut FiletestInstance,
    texture_specs: &TextureSpecs,
    texture_fixtures: &TextureFixtures,
) -> anyhow::Result<Vec<LpvmBuffer>> {
    validate_runtime_texture_fixtures(texture_specs, texture_fixtures)?;
    if texture_specs.is_empty() {
        return Ok(Vec::new());
    }

    let mut keep_alive = Vec::new();
    const ALLOC_ALIGN: usize = 4;

    for (name, _spec) in texture_specs {
        let fixture = texture_fixtures.get(name).ok_or_else(|| {
            anyhow::anyhow!("internal: missing fixture for validated spec {name:?}")
        })?;
        let encoded = encode_texture_fixture(fixture)?;
        let buf = compiled.alloc_shared(encoded.bytes.len(), ALLOC_ALIGN)?;
        unsafe {
            buf.write(0, &encoded.bytes).map_err(|_| {
                anyhow::anyhow!("texture fixture {name:?}: LpvmBuffer::write out of bounds")
            })?;
        }
        let guest = buf.guest_base();
        let ptr = u32::try_from(guest).map_err(|_| {
            anyhow::anyhow!("texture fixture {name:?}: guest pointer {guest} does not fit u32")
        })?;
        let desc = LpsTexture2DDescriptor {
            ptr,
            width: encoded.width,
            height: encoded.height,
            row_stride: encoded.row_stride,
        };
        inst.set_uniform(name.as_str(), &LpsValueF32::Texture2D(desc))
            .map_err(|e| anyhow::anyhow!("set_uniform texture {name:?}: {e}"))?;
        keep_alive.push(buf);
    }

    Ok(keep_alive)
}

/// Validate dimensions, pixel and channel counts, and normalized float ranges; then
/// encode channels as little-endian `u16` in pixel / row order.
pub fn encode_texture_fixture(fixture: &TextureFixture) -> anyhow::Result<EncodedTextureFixture> {
    if fixture.width == 0 || fixture.height == 0 {
        anyhow::bail!(
            "texture fixture {:?}: width and height must be positive (got {}x{})",
            fixture.name,
            fixture.width,
            fixture.height
        );
    }

    let w = u64::from(fixture.width);
    let h = u64::from(fixture.height);
    let expected = w * h;
    if fixture.pixels.len() as u64 != expected {
        anyhow::bail!(
            "texture fixture {:?}: pixel count {} does not match {}x{} ({})",
            fixture.name,
            fixture.pixels.len(),
            fixture.width,
            fixture.height,
            expected
        );
    }

    let format = fixture.format;
    let channels_per_pixel = format.channel_count();
    let bpp = format.bytes_per_pixel();
    let row_stride = fixture.width.checked_mul(bpp as u32).ok_or_else(|| {
        anyhow::anyhow!(
            "texture fixture {:?}: row stride overflow (width={} bpp={})",
            fixture.name,
            fixture.width,
            bpp
        )
    })?;

    let total_bytes = (expected as usize).checked_mul(bpp).ok_or_else(|| {
        anyhow::anyhow!(
            "texture fixture {:?}: total byte size overflow",
            fixture.name
        )
    })?;

    let mut bytes = Vec::new();
    bytes.try_reserve(total_bytes).map_err(|e| {
        anyhow::anyhow!(
            "texture fixture {:?}: could not reserve {} bytes: {e}",
            fixture.name,
            total_bytes
        )
    })?;

    for (i, pixel) in fixture.pixels.iter().enumerate() {
        if pixel.channels.len() != channels_per_pixel {
            anyhow::bail!(
                "texture fixture {:?}: pixel {i} has {} channels, expected {} for {:?}",
                fixture.name,
                pixel.channels.len(),
                channels_per_pixel,
                format
            );
        }
        for ch in &pixel.channels {
            match *ch {
                TextureFixtureChannel::NormalizedFloat(v) => {
                    if !v.is_finite() || v < 0.0 || v > 1.0 {
                        anyhow::bail!(
                            "texture fixture {:?}: pixel {i} has normalized float {v} (expected finite value in 0.0..=1.0)",
                            fixture.name
                        );
                    }
                    bytes.extend_from_slice(&unorm16_le_from_normalized_f32(v));
                }
                TextureFixtureChannel::ExactHex(u) => {
                    bytes.extend_from_slice(&u.to_le_bytes());
                }
            }
        }
    }

    debug_assert_eq!(bytes.len(), total_bytes);

    Ok(EncodedTextureFixture {
        name: fixture.name.clone(),
        width: fixture.width,
        height: fixture.height,
        format,
        bytes,
        row_stride,
    })
}

// --- F32 → unorm16 (matches `lp-shader` render output test helpers) ---

fn q32_word_to_unorm16_le(value_q32: i32) -> [u8; 2] {
    let clamped = value_q32.clamp(0, 65536);
    let unorm = (clamped - (clamped >> 16)) as u16;
    unorm.to_le_bytes()
}

/// Encode a normalized float via `(v * 65536).round` in Q32 space, then the same
/// low-bit extraction used for `FtoUnorm16` / render texture stores.
fn unorm16_le_from_normalized_f32(v: f32) -> [u8; 2] {
    let q = (v * 65536.0).round() as i32;
    q32_word_to_unorm16_le(q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::test_type::TextureFixturePixel;

    fn px_rgba4(r: f32, g: f32, b: f32, a: f32) -> TextureFixturePixel {
        TextureFixturePixel {
            channels: vec![
                TextureFixtureChannel::NormalizedFloat(r),
                TextureFixtureChannel::NormalizedFloat(g),
                TextureFixtureChannel::NormalizedFloat(b),
                TextureFixtureChannel::NormalizedFloat(a),
            ],
        }
    }

    #[test]
    fn texture_fixture_encodes_2x1_rgba16_unorm_floats() {
        let f = TextureFixture {
            name: "t".into(),
            width: 2,
            height: 1,
            format: TextureStorageFormat::Rgba16Unorm,
            pixels: vec![px_rgba4(1.0, 0.0, 0.0, 1.0), px_rgba4(0.0, 1.0, 0.0, 1.0)],
            line_number: 1,
        };
        let out = encode_texture_fixture(&f).expect("encode");
        assert_eq!(out.row_stride, 16);
        assert_eq!(out.bytes.len(), 16);
        // (1,0,0,1) then (0,1,0,1) in unorm16 LE
        assert_eq!(
            out.bytes,
            vec![
                0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, //
                0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0xFF,
            ]
        );
    }

    #[test]
    fn texture_fixture_encodes_exact_hex_little_endian() {
        let f = TextureFixture {
            name: "h".into(),
            width: 1,
            height: 1,
            format: TextureStorageFormat::Rgba16Unorm,
            pixels: vec![TextureFixturePixel {
                channels: vec![
                    TextureFixtureChannel::ExactHex(0x00FF),
                    TextureFixtureChannel::ExactHex(0xFF00),
                    TextureFixtureChannel::ExactHex(0x1234),
                    TextureFixtureChannel::ExactHex(0xABCD),
                ],
            }],
            line_number: 1,
        };
        let out = encode_texture_fixture(&f).expect("encode");
        assert_eq!(
            out.bytes,
            vec![0xFF, 0x00, 0x00, 0xFF, 0x34, 0x12, 0xCD, 0xAB]
        );
    }

    #[test]
    fn texture_fixture_encodes_r16_and_rgb16_channel_counts() {
        let r = TextureFixture {
            name: "r".into(),
            width: 1,
            height: 1,
            format: TextureStorageFormat::R16Unorm,
            pixels: vec![TextureFixturePixel {
                channels: vec![TextureFixtureChannel::NormalizedFloat(0.5)],
            }],
            line_number: 1,
        };
        let out_r = encode_texture_fixture(&r).expect("r16");
        assert_eq!(out_r.bytes.len(), 2);
        assert_eq!(out_r.row_stride, 2);

        let rgb = TextureFixture {
            name: "rgb".into(),
            width: 1,
            height: 1,
            format: TextureStorageFormat::Rgb16Unorm,
            pixels: vec![TextureFixturePixel {
                channels: vec![
                    TextureFixtureChannel::ExactHex(1),
                    TextureFixtureChannel::ExactHex(2),
                    TextureFixtureChannel::ExactHex(3),
                ],
            }],
            line_number: 1,
        };
        let out_rgb = encode_texture_fixture(&rgb).expect("rgb16");
        assert_eq!(out_rgb.bytes, vec![1, 0, 2, 0, 3, 0]);
        assert_eq!(out_rgb.row_stride, 6);
    }

    #[test]
    fn texture_fixture_rejects_pixel_count_mismatch() {
        let f = TextureFixture {
            name: "b".into(),
            width: 2,
            height: 2,
            format: TextureStorageFormat::R16Unorm,
            pixels: vec![TextureFixturePixel {
                channels: vec![TextureFixtureChannel::ExactHex(0)],
            }],
            line_number: 1,
        };
        let e = encode_texture_fixture(&f).expect_err("expected err");
        assert!(e.to_string().contains("pixel count"));
    }

    #[test]
    fn texture_fixture_rejects_channel_count_mismatch() {
        let f = TextureFixture {
            name: "b".into(),
            width: 1,
            height: 1,
            format: TextureStorageFormat::Rgba16Unorm,
            pixels: vec![TextureFixturePixel {
                channels: vec![TextureFixtureChannel::ExactHex(0)],
            }],
            line_number: 1,
        };
        let e = encode_texture_fixture(&f).expect_err("expected err");
        assert!(e.to_string().contains("channels"));
    }

    #[test]
    fn texture_fixture_rejects_out_of_range_normalized_float() {
        let f = TextureFixture {
            name: "b".into(),
            width: 1,
            height: 1,
            format: TextureStorageFormat::R16Unorm,
            pixels: vec![TextureFixturePixel {
                channels: vec![TextureFixtureChannel::NormalizedFloat(1.5)],
            }],
            line_number: 1,
        };
        let e = encode_texture_fixture(&f).expect_err("expected err");
        assert!(e.to_string().contains("1.5") || e.to_string().contains("normalized"));
    }

    #[test]
    fn texture_fixture_row_stride_by_format() {
        let w = 7u32;
        for (fmt, stride) in [
            (TextureStorageFormat::R16Unorm, 7 * 2),
            (TextureStorageFormat::Rgb16Unorm, 7 * 6),
            (TextureStorageFormat::Rgba16Unorm, 7 * 8),
        ] {
            let f = TextureFixture {
                name: "s".into(),
                width: w,
                height: 1,
                format: fmt,
                pixels: (0..w)
                    .map(|_| TextureFixturePixel {
                        channels: vec![TextureFixtureChannel::ExactHex(0); fmt.channel_count()],
                    })
                    .collect(),
                line_number: 1,
            };
            let out = encode_texture_fixture(&f).expect("encode");
            assert_eq!(out.row_stride, stride, "{fmt:?}");
        }
    }

    fn sample_spec(
        format: TextureStorageFormat,
        shape: TextureShapeHint,
    ) -> lps_shared::TextureBindingSpec {
        use lps_shared::{TextureBindingSpec, TextureFilter, TextureWrap};
        TextureBindingSpec {
            format,
            filter: TextureFilter::Nearest,
            wrap_x: TextureWrap::ClampToEdge,
            wrap_y: TextureWrap::ClampToEdge,
            shape_hint: shape,
        }
    }

    #[test]
    fn runtime_validate_errors_on_missing_fixture() {
        use std::collections::BTreeMap;
        let mut specs = BTreeMap::new();
        specs.insert(
            "tex".into(),
            sample_spec(
                TextureStorageFormat::Rgba16Unorm,
                TextureShapeHint::General2D,
            ),
        );
        let fixtures: TextureFixtures = BTreeMap::new();
        let e = validate_runtime_texture_fixtures(&specs, &fixtures).expect_err("missing fixture");
        let s = format!("{e:#}");
        assert!(s.contains("tex") && s.contains("fixture"), "{s}");
    }

    #[test]
    fn runtime_validate_errors_on_extra_fixture() {
        use std::collections::BTreeMap;
        let specs: TextureSpecs = BTreeMap::new();
        let mut fixtures = BTreeMap::new();
        fixtures.insert(
            "orphan".into(),
            TextureFixture {
                name: "orphan".into(),
                width: 1,
                height: 1,
                format: TextureStorageFormat::R16Unorm,
                pixels: vec![TextureFixturePixel {
                    channels: vec![TextureFixtureChannel::ExactHex(0)],
                }],
                line_number: 1,
            },
        );
        let e = validate_runtime_texture_fixtures(&specs, &fixtures).expect_err("extra fixture");
        let s = format!("{e:#}");
        assert!(s.contains("orphan") && s.contains("texture-spec"), "{s}");
    }

    #[test]
    fn runtime_validate_errors_on_format_mismatch() {
        use std::collections::BTreeMap;
        let mut specs = BTreeMap::new();
        specs.insert(
            "tex".into(),
            sample_spec(
                TextureStorageFormat::Rgba16Unorm,
                TextureShapeHint::General2D,
            ),
        );
        let mut fixtures = BTreeMap::new();
        fixtures.insert(
            "tex".into(),
            TextureFixture {
                name: "tex".into(),
                width: 1,
                height: 1,
                format: TextureStorageFormat::R16Unorm,
                pixels: vec![TextureFixturePixel {
                    channels: vec![TextureFixtureChannel::ExactHex(0)],
                }],
                line_number: 1,
            },
        );
        let e = validate_runtime_texture_fixtures(&specs, &fixtures).expect_err("format mismatch");
        let s = format!("{e:#}");
        assert!(s.contains("format"), "{s}");
    }

    #[test]
    fn runtime_validate_errors_on_height_one_mismatch() {
        use std::collections::BTreeMap;
        let mut specs = BTreeMap::new();
        specs.insert(
            "tex".into(),
            sample_spec(TextureStorageFormat::R16Unorm, TextureShapeHint::HeightOne),
        );
        let mut fixtures = BTreeMap::new();
        fixtures.insert(
            "tex".into(),
            TextureFixture {
                name: "tex".into(),
                width: 1,
                height: 2,
                format: TextureStorageFormat::R16Unorm,
                pixels: vec![
                    TextureFixturePixel {
                        channels: vec![TextureFixtureChannel::ExactHex(0)],
                    },
                    TextureFixturePixel {
                        channels: vec![TextureFixtureChannel::ExactHex(0)],
                    },
                ],
                line_number: 1,
            },
        );
        let e = validate_runtime_texture_fixtures(&specs, &fixtures).expect_err("height-one");
        let s = format!("{e:#}");
        assert!(s.contains("height") && s.contains("height-one"), "{s}");
    }
}
