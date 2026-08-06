//! Recognizer for top-level `uniform sampler2D <ident>;` declarations in
//! authored GLSL.
//!
//! Naga's glsl-in has no combined-sampler *type* — `sampler2D` exists only as
//! the Vulkan-style constructor builtin `sampler2D(tex, samp)` — so both
//! compiler tiers rewrite the combined declaration into separated form before
//! parsing. The CPU tier (`lps-frontend::parse`) emits `texture2D` plus a
//! synthesized companion `sampler`; the GPU tier (`lp-gfx-wgpu`) emits
//! `texture2D` alone, because its `texture()` call sites lower to generated
//! `texelFetch` helpers that never bind a sampler object.
//!
//! The two rewrites were once independent scanners and disagreed about *which*
//! declarations to rewrite — the binding-only layout the engine's palette
//! header generates was declined by one tier and died in naga
//! (`docs/defects/2026-08-05-generated-palette-header-dies-on-naga.md`). This
//! module is the single recognizer both tiers consume; emission stays
//! tier-specific, recognition cannot diverge again.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

/// Blank out `//` and `/* */` comments and `#` preprocessor lines
/// (byte-for-byte replacement with spaces, newlines preserved), so byte
/// offsets in the stripped text index the original source.
pub fn strip_comments_and_directives(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    let mut at_line_start = true;
    while i < out.len() {
        match out[i] {
            b'/' if i + 1 < out.len() && out[i + 1] == b'/' => {
                while i < out.len() && out[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'/' if i + 1 < out.len() && out[i + 1] == b'*' => {
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
                while i < out.len() && !(out[i] == b'*' && i + 1 < out.len() && out[i + 1] == b'/')
                {
                    if out[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
                if i + 1 < out.len() {
                    out[i] = b' ';
                    out[i + 1] = b' ';
                    i += 2;
                }
            }
            b'#' if at_line_start => {
                while i < out.len() && out[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'\n' => {
                at_line_start = true;
                i += 1;
                continue;
            }
            b if b.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            _ => {
                at_line_start = false;
                i += 1;
                continue;
            }
        }
    }
    String::from_utf8(out).expect("comment stripping is byte-for-byte on ASCII structure")
}

/// One recognized top-level `uniform sampler2D <ident>;` declaration.
///
/// Spans index the source passed to [`scan_uniform_sampler2d_decls`]
/// (comments are stripped internally; offsets are shared with the original).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sampler2DDecl {
    /// The whole declaration: from the `layout` keyword (or `uniform` when
    /// unqualified) through the terminating `;`.
    pub span: Range<usize>,
    /// The `layout(...)` qualifier, when the author wrote one.
    pub layout: Option<Range<usize>>,
    /// The `sampler2D` type token (what every tier swaps for `texture2D`).
    pub type_token: Range<usize>,
    /// The declared identifier.
    pub ident: Range<usize>,
    /// `set = N` from the layout. Absent means set 0 — the same default naga
    /// itself applies (`ResourceBinding { group: set.unwrap_or(0), .. }`).
    pub set: u32,
}

/// Any appearance of the `sampler2D` token, classified. Non-`Decl` sites are
/// never rewritable; whether they are a hard error (GPU tier) or left for the
/// downstream parser to diagnose (CPU tier) is the caller's policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sampler2DSite {
    /// A clean top-level declaration — the only rewritable shape.
    Decl(Sampler2DDecl),
    /// `uniform sampler2D` followed by something other than `<ident> ;`
    /// (array, multi-declarator). Span is the type token.
    MalformedUniform(Range<usize>),
    /// `sampler2D` outside a uniform declaration (function parameter,
    /// struct member). Span is the type token.
    NonUniform(Range<usize>),
}

/// Result of scanning one GLSL source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sampler2DScan {
    /// Every `sampler2D` token, in source order.
    pub sites: Vec<Sampler2DSite>,
    /// Highest `binding = N` anywhere in the source (every declaration, not
    /// just samplers). Synthesized bindings must start above this so they can
    /// never collide with an authored or generated slot.
    pub max_explicit_binding: Option<u32>,
}

/// Scan `source` for `sampler2D` tokens and classify each one. Comments and
/// preprocessor lines are ignored; returned spans index `source` directly.
pub fn scan_uniform_sampler2d_decls(source: &str) -> Sampler2DScan {
    let stripped = strip_comments_and_directives(source);
    let bytes = stripped.as_bytes();
    let mut sites = Vec::new();
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
        if &stripped[i..end] == "sampler2D" {
            sites.push(classify(&stripped, i..end));
        }
        i = end;
    }
    Sampler2DScan {
        sites,
        max_explicit_binding: max_explicit_binding(&stripped),
    }
}

/// Classify one `sampler2D` token: a rewritable declaration or one of the two
/// unrewritable shapes.
fn classify(stripped: &str, type_token: Range<usize>) -> Sampler2DSite {
    let bytes = stripped.as_bytes();
    // The token before `sampler2D` must be `uniform`.
    let before = stripped[..type_token.start].trim_end();
    let Some(uniform_start) = before
        .strip_suffix("uniform")
        .filter(|rest| rest.is_empty() || !is_ident_byte(rest.as_bytes()[rest.len() - 1]))
        .map(str::len)
    else {
        return Sampler2DSite::NonUniform(type_token);
    };
    // Exactly `<ident> ;` after the type: rejects arrays, multi-declarator
    // lists, and a missing identifier.
    let mut p = type_token.end;
    while p < bytes.len() && bytes[p].is_ascii_whitespace() {
        p += 1;
    }
    if p >= bytes.len() || !(bytes[p] == b'_' || bytes[p].is_ascii_alphabetic()) {
        return Sampler2DSite::MalformedUniform(type_token);
    }
    let ident_start = p;
    while p < bytes.len() && is_ident_byte(bytes[p]) {
        p += 1;
    }
    let ident = ident_start..p;
    while p < bytes.len() && bytes[p].is_ascii_whitespace() {
        p += 1;
    }
    if p >= bytes.len() || bytes[p] != b';' {
        return Sampler2DSite::MalformedUniform(type_token);
    }
    let semi_end = p + 1;

    let layout = leading_layout(stripped, uniform_start);
    let set = layout
        .clone()
        .and_then(|l| parse_layout_set(&stripped[l]))
        .unwrap_or(0);
    let span_start = layout.as_ref().map_or(uniform_start, |l| l.start);
    Sampler2DSite::Decl(Sampler2DDecl {
        span: span_start..semi_end,
        layout,
        type_token,
        ident,
        set,
    })
}

/// `layout(...)` qualifier ending immediately before `uniform_start`
/// (whitespace allowed between), matched backward with paren depth so nested
/// parentheses in the qualifier cannot desynchronize the span.
fn leading_layout(stripped: &str, uniform_start: usize) -> Option<Range<usize>> {
    let before = stripped[..uniform_start].trim_end();
    if !before.ends_with(')') {
        return None;
    }
    let bytes = before.as_bytes();
    let mut depth = 0i32;
    let mut open = None;
    for j in (0..before.len()).rev() {
        match bytes[j] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    open = Some(j);
                    break;
                }
            }
            _ => {}
        }
    }
    let head = stripped[..open?].trim_end();
    let kw_start = head
        .strip_suffix("layout")
        .filter(|rest| rest.is_empty() || !is_ident_byte(rest.as_bytes()[rest.len() - 1]))
        .map(str::len)?;
    Some(kw_start..before.len())
}

/// `set = N` in a layout qualifier's text (`layout ( ... )`). Entries that are
/// not `key = value` (bare qualifiers like `std140`) are skipped — a bare
/// qualifier must not defeat recognition (the defect's masking pattern).
fn parse_layout_set(layout: &str) -> Option<u32> {
    let inner = layout
        .strip_prefix("layout")?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    for part in inner.split(',') {
        let Some((key, val)) = part.split_once('=') else {
            continue;
        };
        if key.trim() == "set" {
            return parse_ascii_u32(val.trim());
        }
    }
    None
}

/// Highest `binding = N` value in the stripped source.
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
        if let Some(value) = parse_ascii_u32(rest.trim_start()) {
            max = Some(max.map_or(value, |m: u32| m.max(value)));
        }
    }
    max
}

/// Leading-digit u32 (`"3"`, or `"3 "` after a trim).
fn parse_ascii_u32(s: &str) -> Option<u32> {
    let end = s
        .as_bytes()
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    s.get(..end)?.parse().ok()
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only_decl(src: &str) -> Sampler2DDecl {
        let scan = scan_uniform_sampler2d_decls(src);
        assert_eq!(scan.sites.len(), 1, "{src}");
        let Sampler2DSite::Decl(d) = scan.sites.into_iter().next().unwrap() else {
            panic!("expected Decl for {src}");
        };
        d
    }

    #[test]
    fn bare_declaration_is_recognized() {
        let src = "uniform sampler2D foo;\n";
        let d = only_decl(src);
        assert_eq!(&src[d.span.clone()], "uniform sampler2D foo;");
        assert_eq!(&src[d.ident.clone()], "foo");
        assert_eq!(d.layout, None);
        assert_eq!(d.set, 0);
    }

    /// The engine's generated spelling (`generate_compute_shader_header`):
    /// binding only, no `set`. Declining this shape was the defect.
    #[test]
    fn binding_only_layout_is_recognized() {
        let src = "layout(binding = 3) uniform sampler2D palette;\n";
        let d = only_decl(src);
        assert_eq!(&src[d.layout.clone().unwrap()], "layout(binding = 3)");
        assert_eq!(d.set, 0);
    }

    #[test]
    fn set_and_binding_layout_parses_set() {
        let d = only_decl("layout(set = 2, binding = 7) uniform sampler2D t;\n");
        assert_eq!(d.set, 2);
    }

    /// A bare qualifier (`std140`) must not defeat recognition, and spacing
    /// between `layout` and `(` is legal GLSL.
    #[test]
    fn bare_qualifier_and_spaced_layout_are_recognized() {
        let d = only_decl("layout (std140, binding = 2) uniform sampler2D t;\n");
        assert!(d.layout.is_some());
        assert_eq!(d.set, 0);
    }

    #[test]
    fn function_parameter_is_non_uniform() {
        let scan = scan_uniform_sampler2d_decls("vec4 f(sampler2D tex) { return vec4(0.0); }\n");
        assert!(matches!(scan.sites[..], [Sampler2DSite::NonUniform(_)]));
    }

    #[test]
    fn array_and_multi_declarator_are_malformed() {
        for src in [
            "uniform sampler2D s[3];\n",
            "uniform sampler2D a, b;\n",
            "uniform sampler2D;\n",
        ] {
            let scan = scan_uniform_sampler2d_decls(src);
            assert!(
                matches!(scan.sites[..], [Sampler2DSite::MalformedUniform(_)]),
                "{src}"
            );
        }
    }

    #[test]
    fn usampler2d_is_not_a_site() {
        let scan = scan_uniform_sampler2d_decls("uniform usampler2D u;\n");
        assert!(scan.sites.is_empty());
    }

    #[test]
    fn comments_and_directives_hide_tokens() {
        let src =
            "// uniform sampler2D a;\n/* uniform sampler2D b; */\n#define S uniform sampler2D c;\n";
        let scan = scan_uniform_sampler2d_decls(src);
        assert!(scan.sites.is_empty());
        assert_eq!(scan.max_explicit_binding, None);
    }

    /// The max scan covers every declaration, sampler or not — synthesized
    /// bindings must clear the whole source's explicit slots.
    #[test]
    fn max_explicit_binding_spans_all_declarations() {
        let src = "layout(binding = 0) uniform float speed;\n\
                   layout(binding = 1) uniform sampler2D palette;\n\
                   layout(binding = 2) uniform float bright;\n";
        let scan = scan_uniform_sampler2d_decls(src);
        assert_eq!(scan.max_explicit_binding, Some(2));
    }
}
