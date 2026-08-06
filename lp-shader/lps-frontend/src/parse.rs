//! GLSL source preparation and Naga parse (`glsl-in`).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use naga::{Module, ShaderStage};

use crate::naga_types::{CompileError, NagaModule, naga_module_from_parsed};

/// LPFX preamble and `#line 1` sent to Naga before the user snippet (same layout as [`compile`]).
const LPFX_PREFIX: &str = concat!(
    "#version 450 core\n",
    include_str!("lpfn_prologue.glsl"),
    "\n#line 1\n",
);

#[inline]
fn is_glsl_id_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// GLSL 4.x allows `const` and `in` in either order for read-only by-value parameters. Naga's
/// glsl-in rejects both `const in T` and `in const T` on parameters; a plain `const T` is the
/// same GLSL (storage defaults to `in`) and is accepted. Strip the redundant `in` in those
/// two word orders so filetests and shaders using explicit `in` can compile.
fn normalize_const_in_param_order(src: &str) -> String {
    let lines: alloc::vec::Vec<&str> = src.lines().collect();
    if lines.is_empty() {
        return if src.is_empty() {
            String::new()
        } else {
            // `src` was only a newline, or a single empty line: preserve
            String::from(src)
        };
    }
    let mut out = String::with_capacity(src.len());
    for (line_idx, line) in lines.iter().enumerate() {
        if line_idx > 0 {
            out.push('\n');
        }
        out.push_str(&normalize_const_in_one_line(line));
    }
    if src.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn split_line_at_comment(line: &str) -> (&str, Option<&str>) {
    let Some(i) = line.find("//") else {
        return (line, None);
    };
    (line.get(..i).unwrap_or(line), line.get(i..))
}

/// `const` (keyword) + `in` (keyword) → `const` (the word `in` is dropped; default storage).
fn try_match_const_then_in(code: &str, start: usize) -> Option<usize> {
    let b = code.as_bytes();
    if start + 5 > b.len() {
        return None;
    }
    if b.get(start..start + 5) != Some(b"const") {
        return None;
    }
    if start > 0 && is_glsl_id_byte(b[start - 1]) {
        return None;
    }
    if start + 5 < b.len() && is_glsl_id_byte(b[start + 5]) {
        return None;
    }
    let mut j = start + 5;
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if j + 2 > b.len() {
        return None;
    }
    if b.get(j..j + 2) != Some(b"in") {
        return None;
    }
    if j + 2 < b.len() && is_glsl_id_byte(b[j + 2]) {
        return None;
    }
    Some(j + 2)
}

/// `in` (keyword) + `const` (keyword) → `const` (drop `in`).
fn try_match_in_then_const(code: &str, start: usize) -> Option<usize> {
    let b = code.as_bytes();
    if start + 2 > b.len() {
        return None;
    }
    if b.get(start..start + 2) != Some(b"in") {
        return None;
    }
    if start > 0 && is_glsl_id_byte(b[start - 1]) {
        return None;
    }
    if start + 2 < b.len() && is_glsl_id_byte(b[start + 2]) {
        return None;
    }
    let mut j = start + 2;
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if j + 5 > b.len() {
        return None;
    }
    if b.get(j..j + 5) != Some(b"const") {
        return None;
    }
    if j + 5 < b.len() && is_glsl_id_byte(b[j + 5]) {
        return None;
    }
    Some(j + 5)
}

fn normalize_const_in_one_line(line: &str) -> String {
    let (code, comment) = split_line_at_comment(line);
    let b = code.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < b.len() {
        if let Some(end) = try_match_const_then_in(code, i) {
            out.push_str("const");
            i = end;
        } else if let Some(end) = try_match_in_then_const(code, i) {
            out.push_str("const");
            i = end;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    if let Some(tail) = comment {
        out.push_str(tail);
    }
    out
}

// --- `uniform sampler2D` → Naga-compatible `layout … uniform texture2D` ---------------------------
//
// Naga’s GLSL-IN does not list `sampler2D` as a built-in type name (`naga::front::glsl::types::parse_type`):
// the lexer feeds `sampler2D` as an identifier, so `uniform sampler2D name;` does not parse.
// LightPlayer’s public surface is still `uniform sampler2D` (classic GLSL), so we rewrite the
// declarations `lps_shared::scan_uniform_sampler2d_decls` recognizes — the recognizer is shared
// with the GPU tier (`lp-gfx-wgpu::texture_lowering`) so the two can never disagree about *which*
// declarations are rewritten (docs/defects/2026-08-05-generated-palette-header-dies-on-naga.md).
// Non-declaration `sampler2D` sites (function parameters, arrays, multi-declarators) pass through
// untouched and get Naga's own diagnostic; the GPU tier makes them hard errors instead.
//
// Naga needs a `(texture2D, sampler)` pair for `texture()`; we synthesize `uniform sampler __lp_samp_X`
// and rewrite `texture(X,` → `texture(sampler2D(X, __lp_samp_X),` after emitting the two uniforms.
//
// Every synthesized binding (companion samplers, and both bindings of an unqualified declaration)
// numbers from one past the source's highest explicit `binding = N`, so it can never collide with
// an authored or generated slot. The numbers are decorative on this path — LP lowering keys globals
// on (name, address space) in declaration order and never reads `gv.binding` (`lower.rs::
// compute_global_layout`) — but collision-free numbering keeps them honest for a reader.

fn rewrite_user_uniform_sampler2d_decls_for_naga(user_snippet: &str) -> String {
    use core::fmt::Write as _;
    use lps_shared::sampler2d_decl::{Sampler2DSite, scan_uniform_sampler2d_decls};

    if user_snippet.is_empty() {
        return String::new();
    }
    let scan = scan_uniform_sampler2d_decls(user_snippet);
    let mut next_binding: u32 = scan.max_explicit_binding.map_or(0, |b| b.saturating_add(1));
    let mut out = String::new();
    let mut cursor = 0usize;
    let mut texture_idents: Vec<String> = Vec::new();
    for site in &scan.sites {
        let Sampler2DSite::Decl(d) = site else {
            continue;
        };
        out.push_str(&user_snippet[cursor..d.span.start]);
        // Indentation for the synthesized companion line: the whitespace run
        // between the declaration and its line start (empty when code precedes).
        let line_start = user_snippet[..d.span.start].rfind('\n').map_or(0, |i| i + 1);
        let lead = &user_snippet[line_start..d.span.start];
        let lead_ws = if lead.chars().all(char::is_whitespace) {
            lead
        } else {
            ""
        };
        let ident = &user_snippet[d.ident.clone()];
        if let Some(lay) = &d.layout {
            // Keep the authored layout verbatim on the texture; the companion
            // joins the same set at the next free binding.
            let lay = &user_snippet[lay.clone()];
            let bind = next_binding;
            next_binding = next_binding.saturating_add(1);
            let _ = write!(
                out,
                "{lay} uniform texture2D {ident};\n{lead_ws}layout(set={set}, binding={bind}) uniform sampler __lp_samp_{ident};",
                set = d.set
            );
        } else {
            let bind = next_binding;
            let bind2 = next_binding.saturating_add(1);
            next_binding = next_binding.saturating_add(2);
            let _ = write!(
                out,
                "layout(set=0, binding={bind}) uniform texture2D {ident};\n{lead_ws}layout(set=0, binding={bind2}) uniform sampler __lp_samp_{ident};"
            );
        }
        cursor = d.span.end;
        texture_idents.push(String::from(ident));
    }
    out.push_str(&user_snippet[cursor..]);
    rewrite_texture_calls_to_use_sampler2d_ctor(&mut out, &texture_idents);
    out
}

fn rewrite_texture_calls_to_use_sampler2d_ctor(out: &mut String, texture_idents: &[String]) {
    if texture_idents.is_empty() {
        return;
    }
    let mut ids: Vec<&str> = texture_idents.iter().map(|s| s.as_str()).collect();
    ids.sort_by_key(|s| usize::MAX - s.len());
    for id in ids {
        let from = format!("texture({id},");
        let to = format!("texture(sampler2D({id}, __lp_samp_{id}),");
        while let Some(i) = out.find(&from) {
            out.replace_range(i..i + from.len(), &to);
        }
    }
}

fn prepend_lpfn_prototypes(source: &str) -> String {
    let source = normalize_const_in_param_order(source);
    let mut s = String::from(LPFX_PREFIX);
    s.push_str(&source);
    s
}

/// 1-based physical line where the user snippet's line 1 begins in sources from
/// [`prepared_glsl_for_compile`] (after `#line 1`, before any synthesized `void main()` suffix).
pub fn user_snippet_first_physical_line() -> usize {
    LPFX_PREFIX.lines().count() + 1
}

/// Full GLSL source passed to Naga: LPFX preamble, user snippet, then optional synthesized
/// `void main() {}` when the user did not define `void main`.
pub fn prepared_glsl_for_compile(user_snippet: &str) -> String {
    let user = rewrite_user_uniform_sampler2d_decls_for_naga(user_snippet);
    let source = prepend_lpfn_prototypes(&user);
    ensure_vertex_entry_point(&source)
}

/// Parse GLSL and collect named function metadata.
pub fn compile(source: &str) -> Result<NagaModule, CompileError> {
    let source = prepared_glsl_for_compile(source);
    let module = parse_glsl(&source)?;
    naga_module_from_parsed(module)
}

/// Naga's GLSL frontend expects a shader entry point. Filetests and snippets only define helpers;
/// append an empty `main` when missing.
fn ensure_vertex_entry_point(source: &str) -> String {
    if glsl_source_declares_main(source) {
        return String::from(source);
    }
    let mut s = String::from(source);
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str("void main() {}\n");
    s
}

fn glsl_source_declares_main(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim_start();
        if t.starts_with("//") {
            return false;
        }
        t.split_whitespace().any(|tok| tok.starts_with("main("))
    })
}

fn parse_glsl(source: &str) -> Result<Module, CompileError> {
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(ShaderStage::Vertex);
    frontend
        .parse(&options, source)
        .map_err(|e| CompileError::Parse(render_parse_errors_at_user_lines(&e, source)))
}

/// Render Naga parse errors with line/column numbers in user-snippet coordinates.
///
/// The `#line 1` directive in [`LPFX_PREFIX`] does not reach Naga's diagnostics:
/// error spans are byte offsets into the parsed source and `emit_to_string`
/// counts physical lines, so an error on user line 8 would render as line 48.
/// Shift every span past the prefix and render against the source tail (the
/// user snippet plus any synthesized `main`). A span inside the prefix itself
/// becomes undefined: the message is kept, the misleading location dropped.
fn render_parse_errors_at_user_lines(
    errors: &naga::front::glsl::ParseErrors,
    source: &str,
) -> String {
    let Some(user_source) = source.strip_prefix(LPFX_PREFIX) else {
        return errors.emit_to_string(source);
    };
    let prefix_len = LPFX_PREFIX.len();
    let remapped: Vec<naga::front::glsl::Error> = errors
        .errors
        .iter()
        .map(|e| {
            let meta = match e.meta.to_range() {
                Some(r) if r.end > prefix_len => naga::Span::new(
                    r.start.saturating_sub(prefix_len) as u32,
                    (r.end - prefix_len) as u32,
                ),
                _ => naga::Span::UNDEFINED,
            };
            naga::front::glsl::Error {
                kind: e.kind.clone(),
                meta,
            }
        })
        .collect();
    naga::front::glsl::ParseErrors::from(remapped).emit_to_string(user_source)
}

#[cfg(test)]
mod error_line_remap_tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn parse_error_reports_user_snippet_line() {
        let src = "vec4 render(vec2 pos) {\n    return vec4(pos, 0.0, 1.0);\n}\nfloat bad = ;\n";
        let Err(err) = compile(src) else {
            panic!("`= ;` must not parse");
        };
        let CompileError::Parse(msg) = err else {
            panic!("expected Parse error, got {err:?}");
        };
        assert!(msg.contains("glsl:4:"), "user line expected:\n{msg}");
        let physical = user_snippet_first_physical_line() + 3;
        assert!(
            !msg.contains(&format!("glsl:{physical}:")),
            "physical line must not leak:\n{msg}"
        );
    }

    #[test]
    fn error_span_inside_prefix_drops_the_location() {
        let composed = prepared_glsl_for_compile("float ok = 1.0;\n");
        let errors = naga::front::glsl::ParseErrors::from(vec![naga::front::glsl::Error {
            kind: naga::front::glsl::ErrorKind::SemanticError("boom".into()),
            meta: naga::Span::new(2, 6),
        }]);
        let msg = render_parse_errors_at_user_lines(&errors, &composed);
        assert!(msg.contains("boom"), "{msg}");
        assert!(
            !msg.contains("glsl:"),
            "no location marker expected:\n{msg}"
        );
    }

    #[test]
    fn non_prefixed_source_renders_unchanged() {
        let src = "float bad = ;\n";
        let errors = naga::front::glsl::ParseErrors::from(vec![naga::front::glsl::Error {
            kind: naga::front::glsl::ErrorKind::SemanticError("boom".into()),
            meta: naga::Span::new(0, 5),
        }]);
        let msg = render_parse_errors_at_user_lines(&errors, src);
        assert!(msg.contains("glsl:1:"), "{msg}");
    }
}

#[cfg(test)]
mod uniform_sampler2d_compat_tests {
    use super::rewrite_user_uniform_sampler2d_decls_for_naga;

    #[test]
    fn injects_default_layout_and_texture2d() {
        let s = "uniform sampler2D foo;\n";
        let o = rewrite_user_uniform_sampler2d_decls_for_naga(s);
        assert_eq!(
            o,
            "layout(set=0, binding=0) uniform texture2D foo;\nlayout(set=0, binding=1) uniform sampler __lp_samp_foo;\n"
        );
    }

    #[test]
    fn preserves_existing_layout_replaces_type_only() {
        let s = "layout(set=0, binding=7) uniform sampler2D bar;\n";
        let o = rewrite_user_uniform_sampler2d_decls_for_naga(s);
        assert_eq!(
            o,
            "layout(set=0, binding=7) uniform texture2D bar;\nlayout(set=0, binding=8) uniform sampler __lp_samp_bar;\n"
        );
    }

    #[test]
    fn second_declaration_gets_next_binding() {
        let s = "uniform sampler2D a;\nuniform sampler2D b;\n";
        let o = rewrite_user_uniform_sampler2d_decls_for_naga(s);
        assert!(o.contains("binding=0) uniform texture2D a"));
        assert!(o.contains("binding=1) uniform sampler __lp_samp_a"));
        assert!(o.contains("binding=2) uniform texture2D b"));
        assert!(o.contains("binding=3) uniform sampler __lp_samp_b"));
    }

    #[test]
    fn does_not_touch_usampler2d() {
        let s = "uniform usampler2D u;\n";
        let o = rewrite_user_uniform_sampler2d_decls_for_naga(s);
        assert_eq!(o, s);
    }

    /// `set` is optional in GLSL (absent means set 0) and nothing in the tree
    /// writes one: `lpc-model`'s `generate_compute_shader_header` emits
    /// `layout(binding = N) uniform sampler2D <name>;` for every palette slot.
    /// Requiring `set` here made the rewrite decline that spelling, so the line
    /// reached Naga verbatim and died as "variable qualifier".
    #[test]
    fn binding_only_layout_is_rewritten() {
        let s = "layout(binding = 3) uniform sampler2D palette;\n";
        let o = rewrite_user_uniform_sampler2d_decls_for_naga(s);
        assert_eq!(
            o,
            "layout(binding = 3) uniform texture2D palette;\nlayout(set=0, binding=4) uniform sampler __lp_samp_palette;\n"
        );
    }

    /// A layout entry with no `=` (a bare qualifier) must not make the whole
    /// declaration decline the rewrite.
    #[test]
    fn bare_layout_qualifier_does_not_defeat_rewrite() {
        let s = "layout(std140, binding = 2) uniform sampler2D palette;\n";
        let o = rewrite_user_uniform_sampler2d_decls_for_naga(s);
        assert!(o.contains("uniform texture2D palette;"), "{o}");
        assert!(
            o.contains("binding=3) uniform sampler __lp_samp_palette;"),
            "{o}"
        );
    }

    /// Synthesized bindings number from one past the source's highest explicit
    /// `binding = N` — across *all* declarations, not just the sampler's own
    /// layout. The old `binding + 1` scheme put the companion on the next
    /// slot's binding (inert here, since LP lowering never reads `gv.binding`,
    /// but a collision nonetheless); this is the GPU tier's scheme.
    #[test]
    fn companion_binding_clears_every_explicit_slot() {
        let s = "layout(binding = 0) uniform float speed;\n\
                 layout(binding = 1) uniform sampler2D palette;\n\
                 layout(binding = 2) uniform float bright;\n";
        let o = rewrite_user_uniform_sampler2d_decls_for_naga(s);
        assert!(o.contains("layout(binding = 1) uniform texture2D palette;"), "{o}");
        assert!(
            o.contains("layout(set=0, binding=3) uniform sampler __lp_samp_palette;"),
            "{o}"
        );
    }

    /// An unqualified declaration alongside explicit slots also numbers past
    /// them (both the texture and its companion).
    #[test]
    fn default_binding_clears_every_explicit_slot() {
        let s = "layout(binding = 4) uniform float speed;\nuniform sampler2D tex;\n";
        let o = rewrite_user_uniform_sampler2d_decls_for_naga(s);
        assert!(o.contains("layout(set=0, binding=5) uniform texture2D tex;"), "{o}");
        assert!(
            o.contains("layout(set=0, binding=6) uniform sampler __lp_samp_tex;"),
            "{o}"
        );
    }
}

/// A palette sampler declared with an explicit binding must reach Naga IR.
///
/// `lpc-model`'s `generate_compute_shader_header` emits `layout(binding = N)
/// uniform sampler2D <name>;` and prepends it to the user's source; the browser
/// CPU tier pins `ShaderFrontend::Naga` (`fw-browser`), so this is a spelling
/// that tier has to compile whether or not the author chose it.
#[cfg(test)]
mod generated_palette_header_compiles_tests {
    use super::compile;

    #[test]
    fn generated_palette_header_compiles_through_naga() {
        let glsl = "layout(binding = 0) uniform vec2 outputSize;\n\
             layout(binding = 1) uniform sampler2D palette;\n\
             vec4 render(vec2 pos) { return texture(palette, vec2(pos.x / outputSize.x, 0.0)); }";
        compile(glsl).expect("generated palette header compiles");
    }

    /// A palette between two other consumed slots: the synthesized companion
    /// sampler numbers past the highest explicit binding in the header, so it
    /// never lands on the next slot's number. (Binding numbers do not drive LP
    /// layout anyway — `compute_global_layout` keys on name + address space in
    /// declaration order — but the header must still reach Naga IR whole.)
    #[test]
    fn palette_between_slots_compiles_with_collision_free_companion_binding() {
        let glsl = "layout(binding = 0) uniform float speed;\n\
             layout(binding = 1) uniform sampler2D palette;\n\
             layout(binding = 2) uniform float bright;\n\
             vec4 render(vec2 pos) { return texture(palette, vec2(pos.x * speed, 0.0)) * bright; }";
        compile(glsl).expect("palette between slots compiles");
    }
}
