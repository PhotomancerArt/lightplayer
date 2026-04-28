//! GLSL source preparation and Naga parse (`glsl-in`).

use alloc::string::String;

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
    let source = prepend_lpfn_prototypes(user_snippet);
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
