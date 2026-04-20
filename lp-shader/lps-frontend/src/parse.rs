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

fn prepend_lpfn_prototypes(source: &str) -> String {
    let mut s = String::from(LPFX_PREFIX);
    s.push_str(source);
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
