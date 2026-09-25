//! Pattern space end to end: what `pos` and the scope intrinsics a shader
//! actually receives, through the real engine, fixture and CPU backend.
//!
//! The rule (ADR `docs/adr/2026-09-24-pattern-space.md`): a shader that
//! declares `"coords": "pattern"` sees the lamps' bounding box, centred at
//! the origin, scaled uniformly so the long side runs −1…1, y up — and in 1D,
//! 0 → 1 from the strand's first lamp to its last — plus `patternExtent`,
//! `patternPitch` and `lampCount`. A shader that does not opt in sees exactly
//! today's pixel `pos` and `outputSize`.
//!
//! The graphics backend here is the real CPU tier (`TargetLpvmGraphics`)
//! behind a thin recorder that copies out every coordinate window and every
//! uniform block the engine hands a program, and the RGBA the program wrote
//! back. The probe shader writes `pos * 0.5 + 0.5` into red/green and
//! `patternExtent.y` into blue, so each sample also proves the program
//! itself — not just the engine — saw the coordinate and the uniform.
//!
//! ```bash
//! cargo test -p lpc-engine --test pattern_space
//! ```

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lp_gfx::{
    GfxError, LpComputeShader, LpGraphics, LpShader, SampleOutHandle, SamplePointsHandle,
    ShaderCompileOptions, ShaderEntrySpace, ShaderSemantics, TextureData, TextureHandle,
};
use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpc_registry::ProjectRegistry;
use lpfs::LpFsStd;
use lps_shared::{LpsValueF32, TextureStorageFormat};

/// Q16.16 one.
const ONE: f64 = 65536.0;
/// The engine's affine is exact to the Q16.16 ulp; the fit and the Q16
/// truncation of the lamp centres add a few more. Far below a pitch.
const COORD_EPS: f64 = 4.0 / ONE;
/// A unorm16 channel's resolution, plus the Q32 program's own rounding.
const CHANNEL_EPS: f64 = 3.0 / 65535.0;

// ---- tests -----------------------------------------------------------------

/// A1 (i): the choker's lamps map to x ∈ [−1, 1] exactly edge to edge, y
/// symmetric about 0 at the half-extent the LAMP box gives (not the canvas),
/// y up.
#[test]
fn choker_lamps_span_x_minus_one_to_one_with_y_from_the_lamp_box() {
    let choker = choker_dir();
    let doc = lpc_mapping::Map2dDoc::from_json(
        &std::fs::read_to_string(choker.join("playful.map2d.json")).unwrap(),
    )
    .expect("choker map2d parses");
    let lamps = lpc_mapping::resolve(&doc).expect("resolve").positions();
    let (min, max) = bounds(&lamps);
    let (width, height) = (max[0] - min[0], max[1] - min[1]);
    assert!(width > height, "the choker is wide: {width} x {height}");
    let expected_half_y = f64::from(height / width);

    let project = Scratch::from(&choker, "choker-pattern");
    project.write_probe_shader_2d("pattern");
    let frame = render_last_frame(project.path());

    let points = frame.points_2d();
    assert_eq!(points.len(), lamps.len(), "one sample per lamp");
    let (pmin, pmax) = bounds_f64(&points);
    assert_close(pmin[0], -1.0, COORD_EPS, "leftmost lamp");
    assert_close(pmax[0], 1.0, COORD_EPS, "rightmost lamp");
    assert_close(pmax[1], expected_half_y, COORD_EPS, "top lamp");
    assert_close(pmin[1], -expected_half_y, COORD_EPS, "bottom lamp");
    assert_close(pmin[1] + pmax[1], 0.0, COORD_EPS, "y symmetric about 0");
    println!(
        "choker: lamp box {width:.3} x {height:.3} doc units -> y half-extent {expected_half_y:.4}; \
         observed x [{:.5}, {:.5}], y [{:.5}, {:.5}]",
        pmin[0], pmax[0], pmin[1], pmax[1]
    );

    // y up: the P's first lamp is its stem's foot, low on the page
    // (largest doc y), so it lands below the centre line.
    let first_doc = lamps[0];
    assert!(
        first_doc[1] > (min[1] + max[1]) / 2.0,
        "P starts low on the page"
    );
    assert!(points[0][1] < 0.0, "y up: {:?}", points[0]);

    let uniforms = frame.pattern_uniforms();
    assert_close(f64::from(uniforms.extent[0]), 1.0, 1e-6, "extent.x");
    assert_close(
        f64::from(uniforms.extent[1]),
        expected_half_y,
        COORD_EPS,
        "extent.y",
    );
    assert_eq!(uniforms.lamp_count, lamps.len() as f32);
    let pitch = mean_consecutive_distance(&points, &frame.strand_starts_choker(&doc));
    assert_close(f64::from(uniforms.pitch), pitch, 1e-3, "pitch");
    println!(
        "choker: patternPitch {} (~{:.1} lamps across)",
        uniforms.pitch,
        2.0 / uniforms.pitch
    );

    frame.assert_program_saw_what_it_was_given(uniforms.extent[1]);
}

/// A1 (ii): a 16×16 grid maps to [−1, 1]², corners exactly, y up.
#[test]
fn a_16x16_grid_maps_to_the_unit_square() {
    let project = Scratch::from(&choker_dir(), "grid-pattern");
    let paths: Vec<Vec<[f32; 2]>> = (0..16)
        .map(|row| {
            (0..16)
                .map(|col| [(col as f32 + 0.5) / 16.0, (row as f32 + 0.5) / 16.0])
                .collect()
        })
        .collect();
    project.write_path_points_fixture(&paths, (16, 16));
    project.write_probe_shader_2d("pattern");
    let frame = render_last_frame(project.path());

    let points = frame.points_2d();
    assert_eq!(points.len(), 256);
    let (pmin, pmax) = bounds_f64(&points);
    for (got, want, what) in [
        (pmin[0], -1.0, "min x"),
        (pmax[0], 1.0, "max x"),
        (pmin[1], -1.0, "min y"),
        (pmax[1], 1.0, "max y"),
    ] {
        assert_close(got, want, COORD_EPS, what);
    }
    // Lamp 0 is the top-left of the authored grid (row 0 is the top in the
    // texture frame): pattern space puts it at (−1, +1).
    assert_close(points[0][0], -1.0, COORD_EPS, "first lamp x");
    assert_close(points[0][1], 1.0, COORD_EPS, "first lamp y (y up)");
    let uniforms = frame.pattern_uniforms();
    assert_eq!(uniforms.extent, [1.0, 1.0]);
    assert_close(f64::from(uniforms.pitch), 2.0 / 15.0, 1e-4, "grid pitch");
    assert_eq!(uniforms.lamp_count, 256.0);
    frame.assert_program_saw_what_it_was_given(uniforms.extent[1]);
}

/// A1 (iii): a straight strip laid out in 2D has a zero-height lamp box —
/// the case a short-side rule would divide by zero on. Extent y is exactly
/// 0, every y is 0, nothing is NaN.
#[test]
fn a_straight_strip_in_2d_has_zero_extent_y_and_no_nan() {
    let project = Scratch::from(&choker_dir(), "strip-pattern");
    let strip: Vec<[f32; 2]> = (0..60).map(|k| [(k as f32 + 0.5) / 60.0, 0.5]).collect();
    project.write_path_points_fixture(&[strip], (60, 8));
    project.write_probe_shader_2d("pattern");
    let frame = render_last_frame(project.path());

    let points = frame.points_2d();
    assert_eq!(points.len(), 60);
    assert!(points.iter().flatten().all(|v| v.is_finite()));
    assert!(points.iter().all(|p| p[1] == 0.0), "every lamp on y = 0");
    let (pmin, pmax) = bounds_f64(&points);
    assert_close(pmin[0], -1.0, COORD_EPS, "first lamp");
    assert_close(pmax[0], 1.0, COORD_EPS, "last lamp");
    let uniforms = frame.pattern_uniforms();
    assert_eq!(uniforms.extent, [1.0, 0.0], "extent.y == 0");
    assert!(uniforms.pitch.is_finite() && !uniforms.pitch.is_nan());
    assert_close(f64::from(uniforms.pitch), 2.0 / 59.0, 1e-4, "strip pitch");
    frame.assert_program_saw_what_it_was_given(uniforms.extent[1]);
}

/// A 1D pattern shader on a strip: `pos` runs 0 → 1 from the first lamp to
/// the last.
#[test]
fn a_one_d_pattern_shader_runs_zero_to_one_along_the_strand() {
    let project = Scratch::from(&choker_dir(), "choker-1d-pattern");
    project.write_probe_shader_1d("pattern");
    let frame = render_last_frame(project.path());

    let ts = frame.points_1d();
    assert_eq!(ts.len(), 73, "the choker's 73 lamps, as a strip");
    let mut sorted = ts.clone();
    sorted.sort_by(f64::total_cmp);
    assert_close(sorted[0], 0.0, COORD_EPS, "first lamp");
    assert_close(sorted[72], 1.0, COORD_EPS, "last lamp");
    for (k, t) in sorted.iter().enumerate() {
        assert_close(*t, k as f64 / 72.0, COORD_EPS, "even steps");
    }
    let uniforms = frame.pattern_uniforms();
    assert_eq!(uniforms.extent, [0.5, 0.0]);
    assert_close(f64::from(uniforms.pitch), 1.0 / 72.0, 1e-6, "1D pitch");
    assert_eq!(uniforms.lamp_count, 73.0);
}

/// Texture-area sampling: a pattern shader's texels are placed through the
/// SAME lamp-box fit as the lamps — one uniform scale, y up, and the
/// fixture's lamp box (not the texture) spans −1…1.
#[test]
fn texture_area_texels_map_through_the_same_lamp_box_fit() {
    let direct = Scratch::from(&choker_dir(), "choker-direct-ref");
    direct.write_probe_shader_2d("pattern");
    let direct_frame = render_last_frame(direct.path());

    let area = Scratch::from(&choker_dir(), "choker-texture-area");
    area.write_probe_shader_2d("pattern");
    area.edit("fixture.json", |text| {
        text.replace("\"sampling\": \"direct\"", "\"sampling\": \"texture_area\"")
    });
    let frame = render_last_frame(area.path());

    let (width, height) = (104usize, 28usize);
    let points = frame.points_2d();
    assert_eq!(points.len(), width * height, "one sample per texel");
    let step_x = points[1][0] - points[0][0];
    let step_y = points[width][1] - points[0][1];
    assert!(step_x > 0.0, "x increases to the right");
    assert_close(step_y, -step_x, 1.0 / ONE, "one uniform scale, y up");
    assert_eq!(
        frame.pattern_uniforms(),
        direct_frame.pattern_uniforms(),
        "same scope, same intrinsics"
    );
    // The lamps span −1…1, so the texture — which frames the whole canvas
    // with margin around the lamps — reaches past them.
    let (pmin, pmax) = bounds_f64(&points);
    assert!(pmin[0] < -1.0 && pmax[0] > 1.0, "{pmin:?} {pmax:?}");
    // And the grid is the lamp fit: the direct run's lamp coordinates are
    // on the texel lattice's affine (checked at the texel nearest each).
    let origin_x = points[0][0] - 0.5 * step_x;
    let origin_y = points[0][1] - 0.5 * step_y;
    for lamp in direct_frame.points_2d() {
        let tx = (lamp[0] - origin_x) / step_x;
        let ty = (lamp[1] - origin_y) / step_y;
        assert!(
            (-0.5..=width as f64 + 0.5).contains(&tx) && (-0.5..=height as f64 + 0.5).contains(&ty),
            "lamp {lamp:?} falls inside the texture ({tx}, {ty})"
        );
    }
    frame.assert_program_saw_what_it_was_given(frame.pattern_uniforms().extent[1]);
}

/// The invariant: a shader that does not opt in — no key, or an explicit
/// `"pixels"` — sees today's pixel `pos` and `outputSize` and nothing else,
/// and the two spellings publish byte-identical output.
#[test]
fn an_opt_out_shader_is_unchanged() {
    let absent = Scratch::from(&choker_dir(), "choker-absent");
    absent.write_probe_shader_2d("absent");
    let absent_frame = render_last_frame(absent.path());
    let absent_bytes = absent_frame.published.clone();

    let pixels = Scratch::from(&choker_dir(), "choker-pixels");
    pixels.write_probe_shader_2d("pixels");
    let pixels_frame = render_last_frame(pixels.path());

    assert!(
        absent_bytes.iter().any(|b| *b != 0),
        "the probe lights lamps"
    );
    assert_eq!(absent_bytes, pixels_frame.published, "absent == \"pixels\"");
    assert_eq!(absent_frame.points, pixels_frame.points);

    // Pixel frame: every lamp inside the 104 × 28 render, `outputSize` the
    // render size, no pattern intrinsics bound.
    let points = absent_frame.points_2d();
    let (pmin, pmax) = bounds_f64(&points);
    assert!(pmin[0] >= 0.0 && pmax[0] <= 104.0 && pmin[1] >= 0.0 && pmax[1] <= 28.0);
    assert!(pmax[0] > 50.0, "pixel units, not pattern units: {pmax:?}");
    let block = absent_frame.last_uniforms();
    assert!(
        matches!(uniform(block, "outputSize"), Some(LpsValueF32::Vec2([w, h])) if *w == 104.0 && *h == 28.0),
        "outputSize is the render size"
    );
    for name in ["patternExtent", "patternPitch", "lampCount"] {
        assert!(
            uniform(block, name).is_none(),
            "{name} bound on an opt-out shader"
        );
    }

    // And the unmodified choker, its own shader untouched, still renders.
    let original = render_last_frame(&choker_dir());
    assert!(original.published.iter().any(|b| *b != 0));
}

/// Q32 at dome scale: the small dome's lamps (5,950 on its dome fixture; the
/// project's 6,310 are split across two fixtures) stay inside ±1 in Q16.16,
/// and the pitch — computed the device's way, one O(n) pass over the
/// streamed coordinates — is non-zero, many ulps wide, and agrees with the
/// same rule computed independently in f64 from the document's own lamps.
// Resolves the dome through the fixture node's own loader, so it needs that gate.
#[cfg(feature = "node-fixture")]
#[test]
fn dome_scale_pitch_is_nonzero_and_q32_safe() {
    use lpc_engine::nodes::fixture::mapping::map2d::mapping_from_map2d_doc;
    use lpc_engine::products::visual::{
        PatternFrame, ScopeGeometry, normalized_f32_to_q16, normalized_q16_to_pixel_q16,
    };
    use lpc_model::nodes::fixture::{MappingRef, mapping_centers};

    let dir = workspace_dir().join("catalog/projects/small-dome/dome");
    let doc = lpc_mapping::Map2dDoc::from_json(
        &std::fs::read_to_string(dir.join("dome.map2d.json")).unwrap(),
    )
    .expect("dome map2d parses");
    let (width, height) = (128, 128);
    let mapping = mapping_from_map2d_doc(&doc, width, height).expect("resolve the dome");
    let mapping = MappingRef::Compact(&mapping);
    let scope = ScopeGeometry::from_mapping(mapping, width, height).expect("lamps");
    assert_eq!(scope.lamp_count, 5950, "the small dome's dome fixture");
    let frame = PatternFrame::two_d(&scope);

    let mut max_abs = 0i32;
    for [x, y] in mapping_centers(mapping) {
        let pixel = [
            normalized_q16_to_pixel_q16(normalized_f32_to_q16(x), width),
            normalized_q16_to_pixel_q16(normalized_f32_to_q16(y), height),
        ];
        let [px, py] = frame.map_point(pixel);
        max_abs = max_abs.max(px.abs()).max(py.abs());
    }
    assert!(
        max_abs <= 65536 + 2,
        "every lamp within ±1 in Q16.16: max |coord| = {max_abs}"
    );

    // The same rule in f64 from doc space: mean consecutive distance within
    // resolver spans, over half the long side of the lamp box.
    let resolved = lpc_mapping::resolve(&doc).expect("resolve");
    let positions = resolved.positions();
    let (min, max) = bounds(&positions);
    let half_long = f64::from((max[0] - min[0]).max(max[1] - min[1])) / 2.0;
    let mut sum = 0.0f64;
    let mut pairs = 0usize;
    for span in &resolved.spans {
        let lamps = &positions[span.start as usize..(span.start + span.count) as usize];
        for pair in lamps.windows(2) {
            let dx = f64::from(pair[1][0] - pair[0][0]);
            let dy = f64::from(pair[1][1] - pair[0][1]);
            sum += (dx * dx + dy * dy).sqrt();
            pairs += 1;
        }
    }
    let expected_pitch = sum / pairs as f64 / half_long;
    println!(
        "small dome: {} lamps, extent {:?}, pitch {} (f64 reference {expected_pitch:.6}, \
         {:.0} Q16 ulps, ~{:.0} lamps across), max |coord| {max_abs}",
        scope.lamp_count,
        frame.extent,
        frame.pitch,
        f64::from(frame.pitch) * ONE,
        2.0 / frame.pitch
    );
    assert!(frame.pitch > 0.0 && frame.pitch.is_finite());
    assert!(
        f64::from(frame.pitch) * ONE > 100.0,
        "pitch is far above the Q16.16 ulp"
    );
    // The device's pitch is taken over pixel-quantized centres (1/65536 px
    // after a 128-px fit); it tracks the f64 rule to well under a percent.
    assert_close(
        f64::from(frame.pitch),
        expected_pitch,
        expected_pitch * 0.01,
        "pitch vs f64 reference",
    );
}

/// The big-dome scale (~30k lamps, 5× the small dome) on a synthetic
/// serpentine grid: the one-pass pitch stays exact and the lamps stay inside
/// ±1 in Q16.16.
#[test]
fn a_30k_lamp_serpentine_keeps_its_pitch_and_range() {
    use lpc_engine::products::visual::{
        PatternFrame, ScopeGeometry, normalized_f32_to_q16, normalized_q16_to_pixel_q16,
    };
    use lpc_model::nodes::fixture::{MappingConfig, MappingRef, PathSpec, mapping_centers};

    let side = 173u32; // 29,929 lamps
    let (width, height) = (256u32, 256u32);
    let rows: Vec<PathSpec> = (0..side)
        .map(|row| {
            let points: Vec<[f32; 2]> = (0..side)
                .map(|col| {
                    let col = if row % 2 == 0 { col } else { side - 1 - col };
                    [
                        (col as f32 + 0.5) / side as f32,
                        (row as f32 + 0.5) / side as f32,
                    ]
                })
                .collect();
            PathSpec::point_list(row * side, points)
        })
        .collect();
    let config = MappingConfig::path_points_vec(rows, 1.0);
    let mapping = MappingRef::Slots(&config);
    let scope = ScopeGeometry::from_mapping(mapping, width, height).expect("lamps");
    assert_eq!(scope.lamp_count, side * side);
    let frame = PatternFrame::two_d(&scope);
    let mut max_abs = 0i32;
    for [x, y] in mapping_centers(mapping) {
        let pixel = [
            normalized_q16_to_pixel_q16(normalized_f32_to_q16(x), width),
            normalized_q16_to_pixel_q16(normalized_f32_to_q16(y), height),
        ];
        let [px, py] = frame.map_point(pixel);
        max_abs = max_abs.max(px.abs()).max(py.abs());
    }
    assert!(max_abs <= 65536 + 2, "max |coord| {max_abs}");
    let expected = 2.0 / f64::from(side - 1);
    println!(
        "30k serpentine: {} lamps, pitch {} (exact {expected:.6}, {:.0} Q16 ulps), max |coord| {max_abs}",
        scope.lamp_count,
        frame.pitch,
        f64::from(frame.pitch) * ONE
    );
    assert_close(f64::from(frame.pitch), expected, expected * 0.005, "pitch");
    assert!(f64::from(frame.pitch) * ONE > 700.0);
}

// ---- the recording backend -------------------------------------------------

#[derive(Default)]
struct Log {
    uniforms: Vec<LpsValueF32>,
    batches: Vec<Batch>,
}

struct Batch {
    space: ShaderEntrySpace,
    points: Vec<i32>,
    samples: Vec<u16>,
}

struct RecordingGraphics {
    inner: Arc<lp_gfx_lpvm::TargetLpvmGraphics>,
    log: Arc<Mutex<Log>>,
}

struct RecordingShader {
    inner: Box<dyn LpShader>,
    graphics: Arc<lp_gfx_lpvm::TargetLpvmGraphics>,
    space: ShaderEntrySpace,
    log: Arc<Mutex<Log>>,
}

impl LpShader for RecordingShader {
    fn render(
        &mut self,
        target: &mut TextureHandle,
        uniforms: &LpsValueF32,
    ) -> Result<(), GfxError> {
        self.log.lock().unwrap().uniforms.push(uniforms.clone());
        self.inner.render(target, uniforms)
    }

    fn bind_uniforms(&mut self, uniforms: &LpsValueF32) -> Result<(), GfxError> {
        self.log.lock().unwrap().uniforms.push(uniforms.clone());
        self.inner.bind_uniforms(uniforms)
    }

    fn sample_rgba16_bound(
        &mut self,
        points: &mut SamplePointsHandle,
        out: &mut SampleOutHandle,
        count: u32,
    ) -> Result<(), GfxError> {
        self.inner.sample_rgba16_bound(points, out, count)?;
        let lanes = match self.space {
            ShaderEntrySpace::TwoD => 2,
            ShaderEntrySpace::OneD => 1,
        };
        let words = self.graphics.read_sample_points(points)?;
        let samples = self.graphics.read_sample_out(out)?;
        self.log.lock().unwrap().batches.push(Batch {
            space: self.space,
            points: words[..count as usize * lanes].to_vec(),
            samples: samples[..count as usize * 4].to_vec(),
        });
        Ok(())
    }

    fn compile_stats(&self) -> Option<lp_gfx::ShaderCompileStats> {
        self.inner.compile_stats()
    }
}

impl LpGraphics for RecordingGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, GfxError> {
        let inner = self.inner.compile_shader(source, options)?;
        Ok(Box::new(RecordingShader {
            inner,
            graphics: Arc::clone(&self.inner),
            space: options.space,
            log: Arc::clone(&self.log),
        }))
    }

    fn compile_compute_shader(
        &self,
        desc: lp_shader::CompileComputeDesc<'_>,
    ) -> Result<Box<dyn LpComputeShader>, GfxError> {
        self.inner.compile_compute_shader(desc)
    }

    fn backend_name(&self) -> &'static str {
        self.inner.backend_name()
    }

    fn native_semantics(&self) -> ShaderSemantics {
        self.inner.native_semantics()
    }

    fn float_semantics(&self) -> ShaderSemantics {
        self.inner.float_semantics()
    }

    fn glsl_frontend(&self) -> lp_shader::ShaderFrontend {
        self.inner.glsl_frontend()
    }

    fn create_render_target(&self, width: u32, height: u32) -> Result<TextureHandle, GfxError> {
        self.inner.create_render_target(width, height)
    }

    fn texture_uniform_value(&self, texture: &TextureHandle) -> Result<LpsValueF32, GfxError> {
        self.inner.texture_uniform_value(texture)
    }

    fn supports_read_back(&self) -> bool {
        self.inner.supports_read_back()
    }

    fn write_sample_points_1d(
        &self,
        points: &mut SamplePointsHandle,
        t_q16: &[i32],
    ) -> Result<(), GfxError> {
        self.inner.write_sample_points_1d(points, t_q16)
    }

    fn read_sample_out(&self, out: &SampleOutHandle) -> Result<Vec<u16>, GfxError> {
        self.inner.read_sample_out(out)
    }

    fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: TextureStorageFormat,
        texels: &[u8],
    ) -> Result<TextureHandle, GfxError> {
        self.inner.create_texture(width, height, format, texels)
    }

    fn write_texture(&self, texture: &mut TextureHandle, texels: &[u8]) -> Result<(), GfxError> {
        self.inner.write_texture(texture, texels)
    }

    fn clear_texture(&self, texture: &mut TextureHandle) -> Result<(), GfxError> {
        self.inner.clear_texture(texture)
    }

    fn blend_textures(
        &self,
        previous: &TextureHandle,
        active: &TextureHandle,
        alpha: f32,
        target: &mut TextureHandle,
    ) -> Result<(), GfxError> {
        self.inner.blend_textures(previous, active, alpha, target)
    }

    fn read_back(&self, texture: &TextureHandle) -> Result<TextureData, GfxError> {
        self.inner.read_back(texture)
    }

    fn read_back_into(&self, texture: &TextureHandle, out: &mut [u8]) -> Result<(), GfxError> {
        self.inner.read_back_into(texture, out)
    }

    fn create_sample_points(&self, count: u32) -> Result<SamplePointsHandle, GfxError> {
        self.inner.create_sample_points(count)
    }

    fn write_sample_points(
        &self,
        points: &mut SamplePointsHandle,
        xy_q16: &[i32],
    ) -> Result<(), GfxError> {
        self.inner.write_sample_points(points, xy_q16)
    }

    fn read_sample_points(&self, points: &SamplePointsHandle) -> Result<Vec<i32>, GfxError> {
        self.inner.read_sample_points(points)
    }

    fn create_sample_out(&self, count: u32) -> Result<SampleOutHandle, GfxError> {
        self.inner.create_sample_out(count)
    }

    fn write_sample_out(&self, out: &mut SampleOutHandle, rgba16: &[u16]) -> Result<(), GfxError> {
        self.inner.write_sample_out(out, rgba16)
    }

    fn read_sample_out_into(&self, out: &SampleOutHandle, dst: &mut [u16]) -> Result<(), GfxError> {
        self.inner.read_sample_out_into(out, dst)
    }

    fn sample_out_data<'a>(&self, out: &'a SampleOutHandle) -> Result<&'a [u16], GfxError> {
        self.inner.sample_out_data(out)
    }

    fn clear_sample_out(&self, out: &mut SampleOutHandle) -> Result<(), GfxError> {
        self.inner.clear_sample_out(out)
    }

    fn sample_points_data_mut<'a>(
        &self,
        points: &'a mut SamplePointsHandle,
    ) -> Result<&'a mut [i32], GfxError> {
        self.inner.sample_points_data_mut(points)
    }

    fn sample_batch_capacity(&self) -> u32 {
        self.inner.sample_batch_capacity()
    }
}

// ---- one frame --------------------------------------------------------------

/// What the program saw on the last frame of a short run.
struct Frame {
    space: Option<ShaderEntrySpace>,
    /// Coordinate words, all batches concatenated.
    points: Vec<i32>,
    samples: Vec<u16>,
    uniforms: Vec<LpsValueF32>,
    /// Every output node's published bytes.
    published: Vec<u8>,
}

#[derive(Debug, PartialEq)]
struct PatternUniforms {
    extent: [f32; 2],
    pitch: f32,
    lamp_count: f32,
}

impl Frame {
    fn points_2d(&self) -> Vec<[f64; 2]> {
        assert_eq!(self.space, Some(ShaderEntrySpace::TwoD));
        self.points
            .chunks_exact(2)
            .map(|p| [f64::from(p[0]) / ONE, f64::from(p[1]) / ONE])
            .collect()
    }

    fn points_1d(&self) -> Vec<f64> {
        assert_eq!(self.space, Some(ShaderEntrySpace::OneD));
        self.points.iter().map(|t| f64::from(*t) / ONE).collect()
    }

    fn last_uniforms(&self) -> &LpsValueF32 {
        self.uniforms.last().expect("a uniform block was bound")
    }

    fn pattern_uniforms(&self) -> PatternUniforms {
        let block = self.last_uniforms();
        let Some(LpsValueF32::Vec2(extent)) = uniform(block, "patternExtent") else {
            panic!("patternExtent not bound: {block:?}");
        };
        let Some(LpsValueF32::F32(pitch)) = uniform(block, "patternPitch") else {
            panic!("patternPitch not bound");
        };
        let Some(LpsValueF32::F32(lamp_count)) = uniform(block, "lampCount") else {
            panic!("lampCount not bound");
        };
        PatternUniforms {
            extent: *extent,
            pitch: *pitch,
            lamp_count: *lamp_count,
        }
    }

    /// The probe writes `pos * 0.5 + 0.5` into red/green and
    /// `patternExtent.y` into blue: the program's own answer must agree with
    /// the coordinates and uniform the engine handed it.
    fn assert_program_saw_what_it_was_given(&self, extent_y: f32) {
        let lanes = match self.space {
            Some(ShaderEntrySpace::TwoD) => 2,
            _ => 1,
        };
        let mut checked = 0;
        for (point, rgba) in self
            .points
            .chunks_exact(lanes)
            .zip(self.samples.chunks_exact(4))
        {
            for lane in 0..lanes {
                let pos = f64::from(point[lane]) / ONE;
                let want = (pos * 0.5 + 0.5).clamp(0.0, 1.0);
                let got = f64::from(rgba[lane]) / 65535.0;
                assert_close(got, want, CHANNEL_EPS, "program pos channel");
            }
            let got_extent = f64::from(rgba[2]) / 65535.0;
            assert_close(
                got_extent,
                f64::from(extent_y),
                CHANNEL_EPS,
                "program extent.y",
            );
            checked += 1;
        }
        assert!(checked > 0);
    }

    /// Strand starts of the choker, one per resolved span (a letter).
    fn strand_starts_choker(&self, doc: &lpc_mapping::Map2dDoc) -> Vec<usize> {
        lpc_mapping::resolve(doc)
            .expect("resolve")
            .spans
            .iter()
            .map(|span| span.start as usize)
            .collect()
    }
}

fn render_last_frame(dir: &Path) -> Frame {
    let log = Arc::new(Mutex::new(Log::default()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(RecordingGraphics {
        inner: Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        )),
        log: Arc::clone(&log),
    });
    let fs = LpFsStd::new(dir.to_path_buf());
    let services = EngineServices::new(TreePath::parse("/probe.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load {}: {e:?}", dir.display()));
    rt.engine_mut().set_graphics(Some(graphics));
    let (mut engine, registry): (Engine, ProjectRegistry) = rt.into_parts();
    // Past the compile-window deferral, then one clean frame.
    for tick in 0..4 {
        engine
            .tick(&registry, 16)
            .unwrap_or_else(|e| panic!("tick {tick}: {e:?}"));
    }
    *log.lock().unwrap() = Log::default();
    engine.tick(&registry, 16).expect("recorded tick");
    let log = std::mem::take(&mut *log.lock().unwrap());
    let space = log.batches.first().map(|batch| batch.space);
    Frame {
        space,
        points: log
            .batches
            .iter()
            .flat_map(|b| b.points.iter().copied())
            .collect(),
        samples: log
            .batches
            .iter()
            .flat_map(|b| b.samples.iter().copied())
            .collect(),
        uniforms: log.uniforms,
        published: published_outputs(&engine),
    }
}

fn published_outputs(engine: &Engine) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in engine.tree().entries() {
        let Some(buffer_id) = engine.runtime_output_sink_buffer_id(entry.id) else {
            continue;
        };
        let Some(buffer) = engine.runtime_buffers().get(buffer_id) else {
            continue;
        };
        out.extend_from_slice(&buffer.value().bytes());
    }
    out
}

// ---- scratch projects ------------------------------------------------------

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    /// A private copy of the project at `src` (top-level files only).
    fn from(src: &Path, name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("lp-pattern-space-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp project dir");
        for entry in std::fs::read_dir(src).expect("read project") {
            let entry = entry.expect("dir entry");
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dir.join(entry.file_name())).expect("copy");
            }
        }
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    fn edit(&self, file: &str, f: impl FnOnce(&str) -> String) {
        let path = self.dir.join(file);
        let before = std::fs::read_to_string(&path).expect("read");
        let after = f(&before);
        assert_ne!(before, after, "{file}: edit changed nothing");
        std::fs::write(path, after).expect("write");
    }

    /// `coords`: `"pattern"`, `"pixels"`, or `"absent"` for no key at all.
    fn write_shader_json(&self, coords: &str, space: Option<&str>) {
        let coords_line = match coords {
            "absent" => String::new(),
            other => format!(",\n  \"coords\": \"{other}\""),
        };
        let space_line = space.map_or_else(String::new, |space| format!(",\n  \"space\": {space}"));
        std::fs::write(
            self.dir.join("shader.json"),
            format!(
                "{{\n  \"kind\": \"Shader\",\n  \"source\": \"shader.glsl\",\n  \"bindings\": {{\n    \"output\": {{ \"target\": \"bus:visual.out\" }}\n  }}{coords_line}{space_line}\n}}\n"
            ),
        )
        .expect("write shader.json");
    }

    fn write_probe_shader_2d(&self, coords: &str) {
        self.write_shader_json(coords, None);
        let extent = if coords == "pattern" {
            "patternExtent.y"
        } else {
            "0.0"
        };
        let pattern_uniforms = if coords == "pattern" {
            "layout(binding = 1) uniform vec2 patternExtent;\n\
             layout(binding = 2) uniform float patternPitch;\n\
             layout(binding = 3) uniform float lampCount;\n"
        } else {
            ""
        };
        let pos = if coords == "pattern" {
            "pos"
        } else {
            // A pixel shader normalizes the old way; the probe only needs
            // to light lamps and differ per lamp.
            "pos / outputSize * 2.0 - 1.0"
        };
        std::fs::write(
            self.dir.join("shader.glsl"),
            format!(
                "layout(binding = 0) uniform vec2 outputSize;\n{pattern_uniforms}\n\
                 vec4 render_2d(vec2 pos) {{\n    vec2 p = {pos};\n    \
                 return vec4(p * 0.5 + 0.5, {extent}, 1.0);\n}}\n"
            ),
        )
        .expect("write shader.glsl");
    }

    fn write_probe_shader_1d(&self, coords: &str) {
        self.write_shader_json(
            coords,
            Some(
                "{ \"kind\": \"OneD\", \"in_2d\": { \"kind\": \"Project\", \"shape\": { \"kind\": \"ExtrudeX\" }, \
                 \"mirror\": { \"kind\": \"Normal\" }, \"flip\": { \"kind\": \"Normal\" } } }",
            ),
        );
        std::fs::write(
            self.dir.join("shader.glsl"),
            "layout(binding = 0) uniform vec2 outputSize;\n\
             layout(binding = 1) uniform vec2 patternExtent;\n\
             layout(binding = 2) uniform float patternPitch;\n\
             layout(binding = 3) uniform float lampCount;\n\n\
             vec4 render_1d(float pos) {\n    return vec4(pos * 0.5 + 0.5, 0.0, patternExtent.y, 1.0);\n}\n",
        )
        .expect("write shader.glsl");
    }

    /// Replace the choker's map2d mapping with hand-authored `PathPoints`
    /// (normalized texture-space centres, one path per strand) at a
    /// `render_size` of `size`.
    fn write_path_points_fixture(&self, paths: &[Vec<[f32; 2]>], size: (u32, u32)) {
        let mut first_channel = 0usize;
        let paths_json: Vec<String> = paths
            .iter()
            .enumerate()
            .map(|(index, points)| {
                let points_json: Vec<String> = points
                    .iter()
                    .enumerate()
                    .map(|(k, [x, y])| format!("\"{k}\": [{x}, {y}]"))
                    .collect();
                let json = format!(
                    "\"{index}\": {{ \"kind\": \"PointList\", \"first_channel\": {first_channel}, \"points\": {{ {} }} }}",
                    points_json.join(", ")
                );
                first_channel += points.len();
                json
            })
            .collect();
        let mapping = format!(
            "\"mapping\": {{ \"kind\": \"PathPoints\", \"paths\": {{ {} }}, \"sample_diameter\": 1.0 }}",
            paths_json.join(", ")
        );
        self.edit("fixture.json", |text| {
            let start = text.find("\"mapping\": {").expect("mapping key");
            let end = start + text[start..].find('}').expect("mapping end") + 1;
            let with_mapping = format!("{}{}{}", &text[..start], mapping, &text[end..]);
            with_mapping
                .replace("\"width\": 104", &format!("\"width\": {}", size.0))
                .replace("\"height\": 28", &format!("\"height\": {}", size.1))
        });
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ---- helpers ---------------------------------------------------------------

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace dir")
        .to_path_buf()
}

fn choker_dir() -> PathBuf {
    workspace_dir().join("catalog/projects/playful-choker")
}

fn uniform<'a>(block: &'a LpsValueF32, name: &str) -> Option<&'a LpsValueF32> {
    let LpsValueF32::Struct { fields, .. } = block else {
        panic!("uniform block is a struct");
    };
    fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

fn bounds(points: &[[f32; 2]]) -> ([f32; 2], [f32; 2]) {
    let mut min = [f32::MAX; 2];
    let mut max = [f32::MIN; 2];
    for p in points {
        for axis in 0..2 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    (min, max)
}

fn bounds_f64(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let mut min = [f64::MAX; 2];
    let mut max = [f64::MIN; 2];
    for p in points {
        for axis in 0..2 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    (min, max)
}

/// Mean distance between consecutive lamps within strands, from the
/// program's own coordinates — the pitch rule, computed independently.
fn mean_consecutive_distance(points: &[[f64; 2]], strand_starts: &[usize]) -> f64 {
    let mut sum = 0.0;
    let mut pairs = 0usize;
    for k in 1..points.len() {
        if strand_starts.contains(&k) {
            continue;
        }
        let dx = points[k][0] - points[k - 1][0];
        let dy = points[k][1] - points[k - 1][1];
        sum += (dx * dx + dy * dy).sqrt();
        pairs += 1;
    }
    sum / pairs as f64
}

fn assert_close(got: f64, want: f64, eps: f64, what: &str) {
    assert!(
        (got - want).abs() <= eps,
        "{what}: got {got}, want {want} (±{eps})"
    );
}
