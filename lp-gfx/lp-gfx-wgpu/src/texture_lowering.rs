//! Lower `texture()` / `texelFetch()` call sites in authored GLSL to
//! generated per-sampler helper functions with LightPlayer sampling
//! semantics (the M5 filtered-sampling decision).
//!
//! # Why manual sampling instead of hardware samplers
//!
//! The CPU tier's `texture()` semantics are defined by
//! `lps-builtins::builtins::texture::sample_ref` (the corpus in
//! `lps-filetests/filetests/texture/` encodes them): the continuous texel
//! coordinate is `uv · extent − 0.5`, nearest selection is GLSL `round`
//! (half away from zero), and **wrap policy applies to the integer texel
//! index** — notably `MirrorRepeat` reflects with period `2·(extent−1)`
//! (edge texels not duplicated: `0 1 2 3 2 1 …`). WebGPU samplers mirror
//! the *normalized coordinate* with period 2 (edge texels duplicated:
//! `0 1 2 3 3 2 1 0 …`), so a hardware `MirrorRepeat` sampler returns a
//! different texel than the CPU tier for coordinates past the edge
//! (`texture_mirror_repeat.glsl` would fail by a whole channel).
//! `texelFetch` likewise clamps out-of-range coordinates to the edge (v0
//! policy, `texelfetch_clamp_bounds.glsl`) while WGSL `textureLoad` is
//! only defined to return *some* in-bounds value or zero for OOB access.
//!
//! Generating the sampling arithmetic in GLSL (option (b) of the M5
//! re-scope) reproduces the CPU tier's semantics exactly in f32, needs no
//! sampler bind slots, and — because everything reduces to `textureLoad`
//! on the `Rgba32Float`/`R32Float` backing — requires **no**
//! `float32-filterable` device feature, so filtered sampling works on
//! every WebGPU adapter. The rejected alternatives: (a) hardware samplers
//! via `float32-filterable` fail the corpus's mirror and OOB-fetch
//! semantics outright and gate on an optional feature; (c) `Rgba16Float`
//! uploads lose the bit-exact `texelFetch` convention *and* still have
//! the sampler semantics problem.
//!
//! # What is rewritten
//!
//! For every sampler leaf named in the compile-time
//! [`TextureBindingSpec`] map:
//!
//! - `texelFetch(name, coord, 0)` → `lp_gfx_fetch_<k>(coord)` — clamps
//!   the integer coordinate to the texture extent, then `texelFetch`
//!   (always in range). Only literal LOD `0` is accepted (CPU parity).
//! - `texture(name, uv)` → `lp_gfx_sample_<k>(uv)` — nearest or bilinear
//!   per the spec's filter, per-axis index wrap per the spec's wrap
//!   modes, `HeightOne` ignores `uv.y` and samples row 0.
//!
//! Sampling a name with no spec is a compile error (CPU parity), as is
//! filtered sampling of an `Rgb16Unorm` binding.

use std::fmt::Write as _;

use lp_gfx::GfxError;
use lp_shader::TextureBindingSpecs;
use lps_shared::{TextureFilter, TextureShapeHint, TextureStorageFormat, TextureWrap};

/// Result of lowering the authored source's texture call sites.
#[derive(Debug)]
pub struct TextureLowering {
    /// Authored source with `texture()` / `texelFetch()` sites rewritten.
    pub rewritten: String,
    /// Shared coordinate helpers (define before the authored source; they
    /// reference no samplers). Empty when no `texture()` site was lowered.
    pub shared_helpers: String,
    /// Prototypes for the per-sampler helpers (before the authored source,
    /// so authored callers resolve; naga glsl-in is declaration-before-use).
    pub helper_prototypes: String,
    /// Per-sampler helper definitions (after the authored source, so the
    /// sampler uniform declarations they reference are in scope).
    pub helper_definitions: String,
}

/// Lower every texture declaration and call site in `authored` against the
/// compile-time spec map.
pub fn lower_texture_calls(
    authored: &str,
    specs: &TextureBindingSpecs,
) -> Result<TextureLowering, GfxError> {
    // Pass 1: `uniform sampler2D name;` → `layout(...) uniform texture2D
    // name;`. naga glsl-in has no `sampler2D` type (same constraint the CPU
    // frontend works around in `lps-frontend::parse`); the separated
    // texture2D global is exactly the WGSL-side shape we bind.
    let stripped = crate::assembly::strip_comments_and_directives(authored);
    let declared = rewrite_sampler_declarations(authored, &stripped)?;

    // Pass 2: rewrite the sampling call sites to generated helpers.
    let stripped = crate::assembly::strip_comments_and_directives(&declared);
    let mut used = Vec::new();
    let rewritten = rewrite(&declared, &stripped, specs, &mut used)?;

    let mut needs_shared = false;
    let mut helper_prototypes = String::new();
    let mut helper_definitions = String::new();
    for entry in &used {
        let (proto, def) = match entry.kind {
            HelperKind::Fetch => fetch_helper(entry),
            HelperKind::Sample => {
                needs_shared = true;
                sample_helper(entry)
            }
        };
        helper_prototypes.push_str(&proto);
        helper_definitions.push_str(&def);
    }

    Ok(TextureLowering {
        rewritten,
        shared_helpers: if needs_shared {
            String::from(SHARED_HELPERS)
        } else {
            String::new()
        },
        helper_prototypes,
        helper_definitions,
    })
}

/// One generated helper: which sampler and which operation.
struct UsedHelper {
    /// Sampler leaf path as written in GLSL (`inputColor`).
    sampler: String,
    /// Stable helper-name index (position in the spec map).
    index: usize,
    spec: lps_shared::TextureBindingSpec,
    kind: HelperKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HelperKind {
    Fetch,
    Sample,
}

impl UsedHelper {
    fn name(&self) -> String {
        let sanitized: String = self
            .sampler
            .chars()
            .map(|c| if c == '.' { '_' } else { c })
            .collect();
        let prefix = match self.kind {
            HelperKind::Fetch => "lp_gfx_fetch",
            HelperKind::Sample => "lp_gfx_sample",
        };
        format!("{prefix}_{}_{sanitized}", self.index)
    }
}

/// Shared coordinate helpers implementing the `sample_ref` semantics in
/// f32: GLSL `round` (half away from zero), euclidean remainder, and the
/// three index-space wrap modes (see the module docs for why `mirror`
/// uses period `2·(n−1)`).
const SHARED_HELPERS: &str = "\
float lp_gfx_round(float x) { return x >= 0.0 ? floor(x + 0.5) : ceil(x - 0.5); }
int lp_gfx_rem_euclid(int i, int n) { int m = i % n; return m < 0 ? m + n : m; }
int lp_gfx_wrap_clamp(int i, int n) { return clamp(i, 0, n - 1); }
int lp_gfx_wrap_repeat(int i, int n) { return lp_gfx_rem_euclid(i, n); }
int lp_gfx_wrap_mirror(int i, int n) {
    if (n <= 1) { return 0; }
    int period = 2 * (n - 1);
    int x = lp_gfx_rem_euclid(i, period);
    return x >= n ? period - x : x;
}
";

fn wrap_fn(wrap: TextureWrap) -> &'static str {
    match wrap {
        TextureWrap::ClampToEdge => "lp_gfx_wrap_clamp",
        TextureWrap::Repeat => "lp_gfx_wrap_repeat",
        TextureWrap::MirrorRepeat => "lp_gfx_wrap_mirror",
    }
}

/// `texelFetch` helper: clamp the integer coordinate to the extent (the
/// v0 out-of-range policy), then load. Returns `(prototype, definition)`.
fn fetch_helper(entry: &UsedHelper) -> (String, String) {
    let name = entry.name();
    let sampler = &entry.sampler;
    let proto = format!("vec4 {name}(ivec2 p);\n");
    let def = format!(
        "vec4 {name}(ivec2 p) {{\n\
         \x20   ivec2 sz = textureSize({sampler}, 0);\n\
         \x20   return texelFetch({sampler}, ivec2(clamp(p.x, 0, sz.x - 1), clamp(p.y, 0, sz.y - 1)), 0);\n\
         }}\n"
    );
    (proto, def)
}

/// `texture()` helper for one sampler: nearest or bilinear at the spec's
/// wrap policy; `HeightOne` ignores `uv.y` and samples row 0.
fn sample_helper(entry: &UsedHelper) -> (String, String) {
    let name = entry.name();
    let sampler = &entry.sampler;
    let wx = wrap_fn(entry.spec.wrap_x);
    let wy = wrap_fn(entry.spec.wrap_y);
    let proto = format!("vec4 {name}(vec2 uv);\n");

    let mut body = String::new();
    let _ = writeln!(body, "vec4 {name}(vec2 uv) {{");
    let _ = writeln!(body, "    ivec2 sz = textureSize({sampler}, 0);");
    match (entry.spec.filter, entry.spec.shape_hint) {
        (TextureFilter::Nearest, TextureShapeHint::General2D) => {
            let _ = writeln!(
                body,
                "    int ix = {wx}(int(lp_gfx_round(uv.x * float(sz.x) - 0.5)), sz.x);\n\
                 \x20   int iy = {wy}(int(lp_gfx_round(uv.y * float(sz.y) - 0.5)), sz.y);\n\
                 \x20   return texelFetch({sampler}, ivec2(ix, iy), 0);"
            );
        }
        (TextureFilter::Nearest, TextureShapeHint::HeightOne) => {
            let _ = writeln!(
                body,
                "    int ix = {wx}(int(lp_gfx_round(uv.x * float(sz.x) - 0.5)), sz.x);\n\
                 \x20   return texelFetch({sampler}, ivec2(ix, 0), 0);"
            );
        }
        (TextureFilter::Linear, TextureShapeHint::General2D) => {
            let _ = writeln!(
                body,
                "    float cx = uv.x * float(sz.x) - 0.5;\n\
                 \x20   float cy = uv.y * float(sz.y) - 0.5;\n\
                 \x20   float fx = floor(cx);\n\
                 \x20   float fy = floor(cy);\n\
                 \x20   int x0 = {wx}(int(fx), sz.x);\n\
                 \x20   int x1 = {wx}(int(fx) + 1, sz.x);\n\
                 \x20   int y0 = {wy}(int(fy), sz.y);\n\
                 \x20   int y1 = {wy}(int(fy) + 1, sz.y);\n\
                 \x20   vec4 c00 = texelFetch({sampler}, ivec2(x0, y0), 0);\n\
                 \x20   vec4 c10 = texelFetch({sampler}, ivec2(x1, y0), 0);\n\
                 \x20   vec4 c01 = texelFetch({sampler}, ivec2(x0, y1), 0);\n\
                 \x20   vec4 c11 = texelFetch({sampler}, ivec2(x1, y1), 0);\n\
                 \x20   return mix(mix(c00, c10, cx - fx), mix(c01, c11, cx - fx), cy - fy);"
            );
        }
        (TextureFilter::Linear, TextureShapeHint::HeightOne) => {
            let _ = writeln!(
                body,
                "    float cx = uv.x * float(sz.x) - 0.5;\n\
                 \x20   float fx = floor(cx);\n\
                 \x20   int x0 = {wx}(int(fx), sz.x);\n\
                 \x20   int x1 = {wx}(int(fx) + 1, sz.x);\n\
                 \x20   vec4 c0 = texelFetch({sampler}, ivec2(x0, 0), 0);\n\
                 \x20   vec4 c1 = texelFetch({sampler}, ivec2(x1, 0), 0);\n\
                 \x20   return mix(c0, c1, cx - fx);"
            );
        }
    }
    body.push_str("}\n");
    (proto, body)
}

/// Rewrite `uniform sampler2D name;` declarations to naga-parseable
/// separated form: `layout(set = 0, binding = K) uniform texture2D name;`.
/// A declaration that already carries an explicit `layout(...)` keeps it
/// (only the type token changes). Any other appearance of `sampler2D`
/// (function parameter, struct member) is a compile error — samplers are
/// uniforms in this dialect.
fn rewrite_sampler_declarations(original: &str, stripped: &str) -> Result<String, GfxError> {
    let bytes = stripped.as_bytes();
    let mut next_binding = max_explicit_binding(stripped).map_or(0, |b| b + 1);
    let mut out = String::new();
    let mut cursor = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if !is_ident_byte(bytes[i]) || (i > 0 && is_ident_byte(bytes[i - 1])) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < bytes.len() && is_ident_byte(bytes[end]) {
            end += 1;
        }
        if &stripped[i..end] != "sampler2D" {
            i = end;
            continue;
        }
        // The token before `sampler2D` must be `uniform`.
        let before = stripped[..i].trim_end();
        let Some(uniform_start) = before
            .strip_suffix("uniform")
            .filter(|rest| rest.is_empty() || !is_ident_byte(rest.as_bytes()[rest.len() - 1]))
            .map(|rest| rest.len())
        else {
            return Err(GfxError::Compile(String::from(
                "sampler2D is only supported as a `uniform sampler2D <name>;` declaration \
                 (optionally with an explicit layout qualifier)",
            )));
        };
        // Reject multi-declarator / array forms after the identifier.
        let mut j = end;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut name_end = j;
        while name_end < bytes.len() && is_ident_byte(bytes[name_end]) {
            name_end += 1;
        }
        let after_name = stripped[name_end..].trim_start();
        if !after_name.starts_with(';') {
            return Err(GfxError::Compile(String::from(
                "sampler2D declarations must be single non-array declarators \
                 (`uniform sampler2D <name>;`)",
            )));
        }

        // Explicit layout present when the text before `uniform` ends with
        // the `)` of a layout qualifier.
        let has_layout = stripped[..uniform_start].trim_end().ends_with(')');
        if has_layout {
            out.push_str(&original[cursor..i]);
            out.push_str("texture2D");
        } else {
            out.push_str(&original[cursor..uniform_start]);
            let _ = write!(
                out,
                "layout(set = 0, binding = {next_binding}) uniform texture2D"
            );
            next_binding += 1;
        }
        cursor = end;
        i = end;
    }
    out.push_str(&original[cursor..]);
    Ok(out)
}

/// Highest `binding = N` value that appears in the stripped source (used to
/// start synthetic sampler bindings after the authored ones).
fn max_explicit_binding(stripped: &str) -> Option<u32> {
    let bytes = stripped.as_bytes();
    let mut max = None;
    let mut i = 0usize;
    while let Some(found) = stripped[i..].find("binding") {
        let start = i + found;
        let end = start + "binding".len();
        i = end;
        let at_boundary = (start == 0 || !is_ident_byte(bytes[start - 1]))
            && (end >= bytes.len() || !is_ident_byte(bytes[end]));
        if !at_boundary {
            continue;
        }
        let rest = stripped[end..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(value) = digits.parse::<u32>() {
            max = Some(max.map_or(value, |m: u32| m.max(value)));
        }
    }
    max
}

/// Rewrite every `texture(...)` / `texelFetch(...)` call in `original`,
/// recursing into argument expressions (nested sampling calls). `stripped`
/// carries the same byte offsets with comments blanked.
fn rewrite(
    original: &str,
    stripped: &str,
    specs: &TextureBindingSpecs,
    used: &mut Vec<UsedHelper>,
) -> Result<String, GfxError> {
    let bytes = stripped.as_bytes();
    let mut out = String::new();
    let mut cursor = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if !is_ident_byte(bytes[i]) || (i > 0 && is_ident_byte(bytes[i - 1])) {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < bytes.len() && is_ident_byte(bytes[end]) {
            end += 1;
        }
        let ident = &stripped[i..end];
        if ident != "texture" && ident != "texelFetch" {
            i = end;
            continue;
        }
        let Some(open) = next_non_ws(bytes, end).filter(|&j| bytes[j] == b'(') else {
            i = end;
            continue;
        };
        let close = matching_paren(stripped, open).ok_or_else(|| {
            GfxError::Compile(format!("unbalanced parentheses in `{ident}(` call"))
        })?;
        let args = split_top_level_args(stripped, open + 1, close);
        let sampler = normalize_path(&stripped[args[0].clone()]);
        let Some(spec_index) = specs.keys().position(|k| *k == sampler) else {
            return Err(GfxError::Compile(format!(
                "`{ident}` samples `{sampler}`, which has no TextureBindingSpec \
                 (every sampler2D uniform leaf needs a compile-time spec)"
            )));
        };
        let spec = *specs
            .get(&sampler)
            .expect("position() found the key, get() must too");

        out.push_str(&original[cursor..i]);
        match ident {
            "texelFetch" => {
                if args.len() != 3 {
                    return Err(GfxError::Compile(format!(
                        "texelFetch on `{sampler}`: expected (sampler, ivec2, lod) arguments"
                    )));
                }
                if stripped[args[2].clone()].trim() != "0" {
                    return Err(GfxError::Compile(format!(
                        "texelFetch on `{sampler}`: only literal LOD 0 is supported"
                    )));
                }
                let helper = record_helper(used, &sampler, spec_index, spec, HelperKind::Fetch);
                let coord = rewrite_arg(
                    &original[args[1].clone()],
                    &stripped[args[1].clone()],
                    specs,
                    used,
                )?;
                let _ = write!(out, "{helper}({coord})");
            }
            _ => {
                if args.len() != 2 {
                    return Err(GfxError::Compile(format!(
                        "texture on `{sampler}`: only texture(sampler2D, vec2) is supported \
                         (no bias / LOD variants)"
                    )));
                }
                if spec.format == TextureStorageFormat::Rgb16Unorm {
                    return Err(GfxError::Compile(format!(
                        "texture `{sampler}`: unsupported format Rgb16Unorm for filtered sampling \
                         (texelFetch only, matching the CPU tier)"
                    )));
                }
                let helper = record_helper(used, &sampler, spec_index, spec, HelperKind::Sample);
                let uv = rewrite_arg(
                    &original[args[1].clone()],
                    &stripped[args[1].clone()],
                    specs,
                    used,
                )?;
                let _ = write!(out, "{helper}({uv})");
            }
        }
        cursor = close + 1;
        i = close + 1;
    }
    out.push_str(&original[cursor..]);
    Ok(out)
}

/// [`rewrite`] for one argument expression, trimmed of the surrounding
/// whitespace the comma split leaves behind.
fn rewrite_arg(
    original: &str,
    stripped: &str,
    specs: &TextureBindingSpecs,
    used: &mut Vec<UsedHelper>,
) -> Result<String, GfxError> {
    Ok(rewrite(original, stripped, specs, used)?.trim().to_string())
}

/// Register a helper use (idempotent) and return its generated name.
fn record_helper(
    used: &mut Vec<UsedHelper>,
    sampler: &str,
    index: usize,
    spec: lps_shared::TextureBindingSpec,
    kind: HelperKind,
) -> String {
    if let Some(existing) = used.iter().find(|u| u.index == index && u.kind == kind) {
        return existing.name();
    }
    let entry = UsedHelper {
        sampler: String::from(sampler),
        index,
        spec,
        kind,
    };
    let name = entry.name();
    used.push(entry);
    name
}

/// Sampler path with all whitespace removed (`params . gradient` →
/// `params.gradient`).
fn normalize_path(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_whitespace()).collect()
}

fn next_non_ws(bytes: &[u8], mut i: usize) -> Option<usize> {
    while i < bytes.len() {
        if !bytes[i].is_ascii_whitespace() {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Byte index of the `)` matching the `(` at `open`.
fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Byte ranges of the top-level comma-separated arguments in `s[start..end)`.
fn split_top_level_args(s: &str, start: usize, end: usize) -> Vec<core::ops::Range<usize>> {
    let bytes = s.as_bytes();
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut arg_start = start;
    for i in start..end {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                args.push(arg_start..i);
                arg_start = i + 1;
            }
            _ => {}
        }
    }
    args.push(arg_start..end);
    args
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_shader::texture_binding;
    use lps_shared::TextureFilter;

    fn specs(entries: &[(&str, lps_shared::TextureBindingSpec)]) -> TextureBindingSpecs {
        let mut map = TextureBindingSpecs::new();
        for (name, spec) in entries {
            map.insert(String::from(*name), *spec);
        }
        map
    }

    fn rgba16_nearest_clamp() -> lps_shared::TextureBindingSpec {
        texture_binding::texture2d(
            TextureStorageFormat::Rgba16Unorm,
            TextureFilter::Nearest,
            TextureWrap::ClampToEdge,
            TextureWrap::ClampToEdge,
        )
    }

    fn lower(authored: &str, specs: &TextureBindingSpecs) -> Result<TextureLowering, GfxError> {
        lower_texture_calls(authored, specs)
    }

    #[test]
    fn texel_fetch_call_site_is_rewritten_to_the_clamping_helper() {
        let authored = "uniform sampler2D inputColor;\n\
                        vec4 f() { return texelFetch(inputColor, ivec2(1, 0), 0); }\n";
        let lowered =
            lower(authored, &specs(&[("inputColor", rgba16_nearest_clamp())])).expect("lowers");
        assert!(
            lowered
                .rewritten
                .contains("lp_gfx_fetch_0_inputColor(ivec2(1, 0))"),
            "{}",
            lowered.rewritten
        );
        assert!(
            lowered
                .helper_definitions
                .contains("textureSize(inputColor, 0)")
        );
        assert!(
            lowered.shared_helpers.is_empty(),
            "fetch-only shaders need no shared coordinate helpers"
        );
    }

    #[test]
    fn texture_call_site_uses_the_spec_wrap_and_filter() {
        let spec = texture_binding::texture2d(
            TextureStorageFormat::Rgba16Unorm,
            TextureFilter::Linear,
            TextureWrap::Repeat,
            TextureWrap::MirrorRepeat,
        );
        let authored = "uniform sampler2D t;\n\
                        vec4 f() { return texture(t, vec2(0.5, 0.5)); }\n";
        let lowered = lower(authored, &specs(&[("t", spec)])).expect("lowers");
        assert!(
            lowered
                .rewritten
                .contains("lp_gfx_sample_0_t(vec2(0.5, 0.5))")
        );
        assert!(lowered.helper_definitions.contains("lp_gfx_wrap_repeat"));
        assert!(lowered.helper_definitions.contains("lp_gfx_wrap_mirror"));
        assert!(lowered.shared_helpers.contains("int lp_gfx_wrap_mirror"));
    }

    #[test]
    fn missing_spec_is_a_compile_error_naming_the_sampler() {
        let authored = "uniform sampler2D mystery;\n\
                        vec4 f() { return texture(mystery, vec2(0.0)); }\n";
        let err = lower(authored, &specs(&[])).expect_err("must fail");
        let GfxError::Compile(message) = err else {
            panic!("expected compile error");
        };
        assert!(message.contains("mystery"), "{message}");
    }

    #[test]
    fn nonzero_lod_is_rejected() {
        let authored = "uniform sampler2D t;\n\
                        vec4 f() { return texelFetch(t, ivec2(0), 1); }\n";
        let err = lower(authored, &specs(&[("t", rgba16_nearest_clamp())])).expect_err("must fail");
        let GfxError::Compile(message) = err else {
            panic!("expected compile error");
        };
        assert!(message.contains("LOD 0"), "{message}");
    }

    #[test]
    fn filtered_rgb16_is_rejected_like_the_cpu_tier() {
        let spec = texture_binding::texture2d(
            TextureStorageFormat::Rgb16Unorm,
            TextureFilter::Nearest,
            TextureWrap::ClampToEdge,
            TextureWrap::ClampToEdge,
        );
        let authored = "uniform sampler2D t;\n\
                        vec4 f() { return texture(t, vec2(0.0)); }\n";
        let err = lower(authored, &specs(&[("t", spec)])).expect_err("must fail");
        let GfxError::Compile(message) = err else {
            panic!("expected compile error");
        };
        assert!(
            message.contains("t") && message.contains("Rgb16Unorm"),
            "{message}"
        );
    }

    #[test]
    fn nested_sampling_calls_rewrite_inner_arguments() {
        let authored = "uniform sampler2D t;\n\
                        vec4 f() { return texture(t, texelFetch(t, ivec2(0), 0).xy); }\n";
        let lowered = lower(authored, &specs(&[("t", rgba16_nearest_clamp())])).expect("lowers");
        assert!(
            lowered
                .rewritten
                .contains("lp_gfx_sample_1_t(lp_gfx_fetch_1_t(ivec2(0)).xy)")
                || lowered
                    .rewritten
                    .contains("lp_gfx_sample_0_t(lp_gfx_fetch_0_t(ivec2(0)).xy)"),
            "{}",
            lowered.rewritten
        );
    }

    #[test]
    fn texture_size_and_other_identifiers_are_untouched() {
        let authored = "uniform sampler2D t;\n\
                        vec4 f() { ivec2 s = textureSize(t, 0); return vec4(s, 0, 1); }\n\
                        vec4 mytexture(vec2 p) { return vec4(p, 0.0, 1.0); }\n";
        let lowered = lower(authored, &specs(&[("t", rgba16_nearest_clamp())])).expect("lowers");
        assert!(lowered.rewritten.contains("textureSize(t, 0)"));
        assert!(lowered.rewritten.contains("mytexture(vec2 p)"));
    }

    #[test]
    fn call_sites_inside_comments_are_ignored() {
        let authored = "uniform sampler2D t;\n\
                        // texture(t, vec2(0.0)) in a comment\n\
                        vec4 f() { return vec4(0.0); }\n";
        let lowered = lower(authored, &specs(&[("t", rgba16_nearest_clamp())])).expect("lowers");
        assert!(lowered.helper_prototypes.is_empty());
        assert!(
            lowered
                .rewritten
                .contains("// texture(t, vec2(0.0)) in a comment")
        );
    }
}
