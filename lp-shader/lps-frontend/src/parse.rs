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
// LightPlayer’s public surface is still `uniform sampler2D` (classic GLSL), so we rewrite **only**
// simple, top-level, single lines of the form (optional `layout(…)`)`uniform sampler2D <ident>;`
// to use `texture2D` and, when there is no `layout` yet, a synthetic `layout(set=0, binding=n)`.
//
// **Not** rewritten here (must keep using `texture2D` + explicit `layout` or fix the grammar later):
// - `usampler2D` / `isampler2D`, `sampler2DShadow`, arrays (`uniform sampler2D s[3];`), multiple
//   declarators (`uniform sampler2D a, b;`), or precision/interpolation between `uniform` and the type.
//
// Naga needs a `(texture2D, sampler)` pair for `texture()`; we synthesize `uniform sampler __lp_samp_X`
// and rewrite `texture(X,` → `texture(sampler2D(X, __lp_samp_X),` after emitting the two uniforms.

fn rewrite_user_uniform_sampler2d_decls_for_naga(user_snippet: &str) -> String {
    if user_snippet.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut next_default_binding: u32 = 0;
    let mut texture_idents: Vec<String> = Vec::new();
    for chunk in user_snippet.split_inclusive('\n') {
        let (line, nl) = if let Some(s) = chunk.strip_suffix('\n') {
            (s, "\n")
        } else {
            (chunk, "")
        };
        if let Some((rew, id)) =
            try_rewrite_top_level_uniform_sampler2d_line(line, &mut next_default_binding)
        {
            if let Some(id) = id {
                texture_idents.push(id);
            }
            out.push_str(&rew);
        } else {
            out.push_str(line);
        }
        out.push_str(nl);
    }
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

/// Parse `layout(set=…, binding=…)` (allows spaces around `=`). Returns `(set, binding)`.
fn parse_glsl_layout_set_binding(layout: &str) -> Option<(u32, u32)> {
    let inner = layout.strip_prefix("layout(")?.strip_suffix(')')?.trim();
    let mut set_v: Option<u32> = None;
    let mut bind_v: Option<u32> = None;
    for part in inner.split(',') {
        let p = part.trim();
        let (key, val) = p.split_once('=')?;
        let key = key.trim();
        let val = val.trim();
        match key {
            "set" => set_v = parse_ascii_u32(val),
            "binding" => bind_v = parse_ascii_u32(val),
            _ => {}
        }
    }
    Some((set_v?, bind_v?))
}

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

fn try_rewrite_top_level_uniform_sampler2d_line(
    line: &str,
    next_default_binding: &mut u32,
) -> Option<(String, Option<String>)> {
    // Ignore line comments for shape matching; comments are not preserved in rewritten line.
    let code = line.split_once("//").map(|(a, _)| a).unwrap_or(line);
    let lead_ws = &line[..line.len() - line.trim_start().len()];
    let t = code.trim();
    if t.is_empty() {
        return None;
    }

    let (layout_str, rem) = parse_optional_leading_layout(t)?;
    if rem.is_empty() {
        return None;
    }
    // Parse: `uniform` `sampler2D` <ident> `;` from `rem`
    let b = rem.as_bytes();
    let w_at = |i: usize, w: &str| -> bool {
        i + w.len() <= b.len() && &b[i..i + w.len()] == w.as_bytes()
    };
    let mut p = 0usize;
    while p < b.len() && b[p].is_ascii_whitespace() {
        p += 1;
    }
    if !w_at(p, "uniform") {
        return None;
    }
    p += "uniform".len();
    if p < b.len() {
        let c = b[p];
        if c == b'_' || c.is_ascii_alphanumeric() {
            return None;
        }
    }
    while p < b.len() && b[p].is_ascii_whitespace() {
        p += 1;
    }
    if !w_at(p, "sampler2D") {
        return None;
    }
    p += "sampler2D".len();
    if p < b.len() {
        let c = b[p];
        if c == b'_' || c.is_ascii_alphanumeric() {
            return None; // e.g. usampler2D
        }
    }
    while p < b.len() && b[p].is_ascii_whitespace() {
        p += 1;
    }
    // ident
    if p >= b.len() {
        return None;
    }
    if !(b[p] == b'_' || b[p].is_ascii_alphabetic()) {
        return None;
    }
    let id0 = p;
    let mut id1 = id0 + 1;
    while id1 < b.len() && (b[id1] == b'_' || b[id1].is_ascii_alphanumeric()) {
        id1 += 1;
    }
    let ident = rem.get(id0..id1)?;
    while id1 < b.len() && b[id1].is_ascii_whitespace() {
        id1 += 1;
    }
    if id1 >= b.len() || b[id1] != b';' {
        return None; // multi-decl, array, or trailing junk
    }
    id1 += 1;
    while id1 < b.len() && b[id1].is_ascii_whitespace() {
        id1 += 1;
    }
    if id1 != b.len() {
        return None;
    }

    let samp = format!("__lp_samp_{ident}");
    let new_core = if let Some(lay) = layout_str {
        let (set, bind) = parse_glsl_layout_set_binding(lay)?;
        let bind2 = bind.checked_add(1)?;
        format!(
            "{lay} uniform texture2D {ident};\n{lead_ws}layout(set={set}, binding={bind2}) uniform sampler {samp};"
        )
    } else {
        let bind = *next_default_binding;
        *next_default_binding = next_default_binding.saturating_add(2);
        let bind2 = bind.checked_add(1)?;
        format!(
            "layout(set=0, binding={bind}) uniform texture2D {ident};\n{lead_ws}layout(set=0, binding={bind2}) uniform sampler {samp};"
        )
    };
    let mut s = String::new();
    s.push_str(lead_ws);
    s.push_str(&new_core);
    // Restore line // comment if any
    if let Some((_, c)) = line.split_once("//") {
        s.push_str("//");
        s.push_str(c);
    }
    Some((s, Some(String::from(ident))))
}

/// `t` is trimmed line code without a line `//` comment.
/// On success, returns `Some((optional layout "layout(…)" slice, text after the layout for uniform))`.
/// `None` if `layout(…` is unclosed (invalid GLSL); caller skips rewriting that line.
fn parse_optional_leading_layout(t: &str) -> Option<(Option<&str>, &str)> {
    let s = t.trim_start();
    if !s.starts_with("layout") {
        return Some((None, t));
    }
    if s.as_bytes()
        .get(6)
        .is_some_and(|c| *c == b'_' || c.is_ascii_alphanumeric())
    {
        // e.g. `layout2` — not a `layout` qualifier
        return Some((None, t));
    }
    let open = s.find('(')?;
    let from_open = s.get(open..)?;
    let mut depth = 0i32;
    for (i, c) in from_open.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                let end = open + i + 1;
                let layout = s.get(0..end)?;
                let after = s.get(end..).unwrap_or("").trim_start();
                return Some((Some(layout), after));
            }
        }
    }
    None
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
        .map_err(|e| CompileError::Parse(e.emit_to_string(source)))
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
}
