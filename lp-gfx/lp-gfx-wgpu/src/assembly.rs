//! Assemble the fragment-stage GLSL compilation unit for the GPU tier.
//!
//! The GPU path forks from the CPU path at the GLSL source (ADR
//! `docs/adr/2026-07-09-gpu-forks-at-glsl.md`): the same authored source the
//! device compiles is spliced with
//!
//! 1. the **canonical lpfn prelude** — every canonical GLSL source
//!    (`lps_builtins::CANONICAL_GLSL`) that defines an `lpfn_*` name the
//!    authored code references, plus transitive dependencies, in manifest
//!    (dependency) order. Driving naga `glsl-in` directly means `lpfn_` is
//!    not a reserved prefix here, so the prelude functions resolve as plain
//!    local GLSL functions with canonical float semantics;
//! 2. **generated prototypes** for every authored function — naga `glsl-in`
//!    resolves calls in source order (declaration-before-use), while the
//!    engine's `lps-glsl` frontend accepts out-of-order definitions, so
//!    authored content may rely on it;
//! 3. **texture call lowering** ([`crate::texture_lowering`]) — every
//!    `texture()` / `texelFetch()` site on a spec'd sampler is rewritten to
//!    a generated helper implementing the CPU tier's sampling semantics
//!    (index-space wrap, `texelFetch` edge clamp) over `textureLoad`;
//! 4. a **generated fragment `main()`** wrapping
//!    `render(floor(gl_FragCoord.xy))` — matching the CPU path's `pos`
//!    convention (the synthesised render-texture loop passes integer pixel
//!    coordinates without a half-pixel offset).
//!
//! Pixel shaders are self-contained sources (uniforms declared in the
//! authored text); the engine's generated-header machinery is
//! compute-shader-only, so there is no shared header to reuse here.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use lp_gfx::GfxError;
use lp_shader::TextureBindingSpecs;
use lps_builtins::canonical_glsl::{CANONICAL_GLSL, CanonicalGlsl};

use crate::texture_lowering::lower_texture_calls;

/// Name of the generated fragment output variable.
const FRAG_OUT: &str = "lp_gfx_frag_color";

/// Assemble the full fragment-stage GLSL for an authored pixel shader.
///
/// `textures` is the compile-time [`lps_shared::TextureBindingSpec`] map
/// keyed by sampler uniform leaf path; sampling call sites are lowered
/// against it (a sampled name without a spec is a compile error).
pub fn assemble_fragment_glsl(
    authored: &str,
    textures: &TextureBindingSpecs,
) -> Result<String, GfxError> {
    let lowered = lower_texture_calls(authored, textures)?;

    let mut out = String::from("#version 450 core\n");
    out.push_str(&assemble_prelude(authored));
    out.push_str(&lowered.shared_helpers);
    out.push_str(&lowered.helper_prototypes);
    out.push_str(&authored_prototypes(authored));
    out.push_str(&lowered.rewritten);
    // Helper definitions come after the authored text so the sampler
    // uniform declarations they reference are already in scope for naga's
    // declaration-before-use resolution (call sites resolve through the
    // prototypes spliced above).
    out.push('\n');
    out.push_str(&lowered.helper_definitions);
    let _ = write!(
        out,
        "\nlayout(location = 0) out vec4 {FRAG_OUT};\n\
         void main() {{\n    {FRAG_OUT} = render(floor(gl_FragCoord.xy));\n}}\n"
    );
    Ok(out)
}

/// Build the canonical lpfn prelude for an authored source: the
/// concatenation of every needed canonical source, dependency-ordered.
/// Returns an empty string when the shader references no lpfn builtins.
pub fn assemble_prelude(authored: &str) -> String {
    let referenced = lpfn_references(authored);
    let mut needed: BTreeSet<&'static str> = BTreeSet::new();
    for entry in CANONICAL_GLSL {
        if defined_lpfn_names(entry)
            .iter()
            .any(|name| referenced.contains(name.as_str()))
        {
            include_with_deps(entry.name, &mut needed);
        }
    }

    let mut out = String::new();
    for entry in CANONICAL_GLSL {
        if needed.contains(entry.name) {
            out.push_str("// canonical builtin: ");
            out.push_str(entry.path);
            out.push('\n');
            out.push_str(entry.source);
            out.push('\n');
        }
    }
    out
}

/// Generate one-line prototypes for every function defined at the top level
/// of `authored`, closing naga glsl-in's declaration-before-use gap for
/// out-of-order authored functions. `main` is never prototyped (authored
/// pixel shaders have no `main`; the wrapper provides it).
///
/// naga glsl-in assigns each function its arena slot at its **first
/// declaration**, and validation requires callees to precede callers in the
/// arena. Prototypes are therefore emitted in callee-first (topological)
/// order of the authored call graph (GLSL forbids recursion, so it is a
/// DAG; a textual false-positive cycle degrades to definition order).
pub fn authored_prototypes(authored: &str) -> String {
    let clean = strip_comments_and_directives(authored);
    let functions = collect_functions(&clean);

    // Call graph on function *names* (overloads share a node): name → names
    // it references (identifier followed by `(` inside any overload body).
    let names: Vec<&str> = {
        let mut names = Vec::new();
        for function in &functions {
            if !names.contains(&function.name.as_str()) {
                names.push(function.name.as_str());
            }
        }
        names
    };
    let deps_of = |name: &str| -> Vec<&str> {
        let mut deps = Vec::new();
        for function in functions.iter().filter(|f| f.name == name) {
            for other in &names {
                if *other != name
                    && !deps.contains(other)
                    && references_call(&clean[function.body.clone()], other)
                {
                    deps.push(other);
                }
            }
        }
        deps
    };

    // Depth-first callee-first emission; `visiting` breaks textual
    // false-positive cycles (real recursion is invalid GLSL anyway).
    let mut out = String::new();
    let mut emitted: BTreeSet<&str> = BTreeSet::new();
    let mut visiting: Vec<&str> = Vec::new();
    fn visit<'a>(
        name: &'a str,
        deps_of: &dyn Fn(&str) -> Vec<&'a str>,
        functions: &[AuthoredFunction],
        emitted: &mut BTreeSet<&'a str>,
        visiting: &mut Vec<&'a str>,
        out: &mut String,
    ) {
        if emitted.contains(name) || visiting.contains(&name) {
            return;
        }
        visiting.push(name);
        for dep in deps_of(name) {
            visit(dep, deps_of, functions, emitted, visiting, out);
        }
        visiting.pop();
        emitted.insert(name);
        for function in functions.iter().filter(|f| f.name == name) {
            out.push_str(&function.signature);
            out.push_str(";\n");
        }
    }
    for name in &names {
        visit(
            name,
            &deps_of,
            &functions,
            &mut emitted,
            &mut visiting,
            &mut out,
        );
    }
    out
}

/// One top-level function definition found in comment-stripped source.
struct AuthoredFunction {
    name: String,
    /// Whitespace-normalized signature (sans `;`).
    signature: String,
    /// Byte range of the body (between the outermost braces).
    body: core::ops::Range<usize>,
}

/// Scan top-level statements for function definitions, capturing signatures
/// and body spans.
fn collect_functions(clean: &str) -> Vec<AuthoredFunction> {
    let bytes = clean.as_bytes();
    let mut functions = Vec::new();
    let mut depth = 0usize;
    let mut stmt_start = 0usize;
    let mut body_start = 0usize;
    let mut pending: Option<(String, String)> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 {
                    let segment = clean[stmt_start..i].trim();
                    pending = function_signature(segment);
                    body_start = i + 1;
                }
                depth += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some((name, signature)) = pending.take() {
                        functions.push(AuthoredFunction {
                            name,
                            signature,
                            body: body_start..i,
                        });
                    }
                    stmt_start = i + 1;
                }
            }
            b';' if depth == 0 => {
                stmt_start = i + 1;
            }
            _ => {}
        }
    }
    functions
}

/// True when `body` contains a call-shaped reference `name(` at an
/// identifier boundary.
fn references_call(body: &str, name: &str) -> bool {
    let bytes = body.as_bytes();
    let needle = name.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        let at_boundary = i == 0 || !is_ident_byte(bytes[i - 1]);
        if at_boundary && &bytes[i..i + needle.len()] == needle {
            let mut j = i + needle.len();
            if j < bytes.len() && !is_ident_byte(bytes[j]) {
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'(' {
                    return true;
                }
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    false
}

/// All `lpfn_*` identifiers referenced in a GLSL source (at identifier
/// boundaries; comments are not stripped — over-inclusion only pads the
/// prelude, and unreferenced prelude functions cost WGSL size only).
fn lpfn_references(src: &str) -> BTreeSet<String> {
    let bytes = src.as_bytes();
    let mut refs = BTreeSet::new();
    let mut i = 0;
    // Byte-wise scan (identifiers are ASCII; comments may contain UTF-8).
    while i + 5 <= bytes.len() {
        let at_boundary = i == 0 || !is_ident_byte(bytes[i - 1]);
        if at_boundary && &bytes[i..i + 5] == b"lpfn_" {
            let mut end = i + 5;
            while end < bytes.len() && is_ident_byte(bytes[end]) {
                end += 1;
            }
            refs.insert(String::from_utf8_lossy(&bytes[i..end]).into_owned());
            i = end;
        } else {
            i += 1;
        }
    }
    refs
}

/// The `lpfn_*` function names a canonical source defines (declarator scan:
/// a return type keyword followed by an `lpfn_` identifier and `(`).
fn defined_lpfn_names(entry: &CanonicalGlsl) -> Vec<String> {
    const RETURN_TYPES: &[&str] = &[
        "float", "vec2", "vec3", "vec4", "int", "uint", "uvec2", "uvec3", "uvec4", "bool", "void",
    ];
    let mut names = Vec::new();
    for line in entry.source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = RETURN_TYPES
            .iter()
            .find_map(|ty| strip_keyword(trimmed, ty))
        else {
            continue;
        };
        if !rest.starts_with("lpfn_") {
            continue;
        }
        let end = rest
            .as_bytes()
            .iter()
            .position(|&b| !is_ident_byte(b))
            .unwrap_or(rest.len());
        if rest[end..].trim_start().starts_with('(') {
            names.push(rest[..end].to_string());
        }
    }
    names
}

/// Add `name` and its transitive deps to `needed`.
fn include_with_deps(name: &'static str, needed: &mut BTreeSet<&'static str>) {
    if !needed.insert(name) {
        return;
    }
    if let Some(entry) = CANONICAL_GLSL.iter().find(|e| e.name == name) {
        for dep in entry.deps {
            include_with_deps(dep, needed);
        }
    }
}

/// If `segment` (top-level text preceding a `{`) is a function definition
/// header, return `(name, whitespace-normalized signature)` (sans `;`).
fn function_signature(segment: &str) -> Option<(String, String)> {
    if segment.is_empty() || segment.starts_with("struct") {
        return None;
    }
    let open = segment.find('(')?;
    if !segment.ends_with(')') {
        return None;
    }
    // Before the parameter list: at least a return type and a function name,
    // and the name must be a plain identifier (rejects e.g. `layout(...)`).
    let head: Vec<&str> = segment[..open].split_whitespace().collect();
    let name = head.last()?;
    if head.len() < 2 || !is_identifier(name) || *name == "main" {
        return None;
    }
    let signature = segment.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(((*name).to_string(), signature))
}

/// Blank out `//` and `/* */` comments and `#` preprocessor lines
/// (byte-for-byte replacement with spaces, newlines preserved).
pub(crate) fn strip_comments_and_directives(src: &str) -> String {
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

fn strip_keyword<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(keyword)?;
    // Must be followed by whitespace (a real declarator, not a prefix match).
    let rest = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('\t'))?;
    Some(rest.trim_start())
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty() && !s.as_bytes()[0].is_ascii_digit() && s.bytes().all(is_ident_byte)
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prelude_for_worley_includes_hash_dependency() {
        let prelude = assemble_prelude("float v = lpfn_worley(p, 0u);");
        assert!(prelude.contains("uint lpfn_hash("));
        assert!(prelude.contains("float lpfn_worley(vec2 p, uint seed)"));
    }

    #[test]
    fn prelude_orders_dependencies_before_dependents() {
        let prelude = assemble_prelude("float v = lpfn_fbm(p, 3, 0u);");
        let hash_at = prelude.find("uint lpfn_hash(").expect("hash included");
        let snoise_at = prelude
            .find("float lpfn_snoise(vec2 p, uint seed)")
            .expect("snoise2 included");
        let fbm_at = prelude
            .find("float lpfn_fbm(vec2 p, int octaves, uint seed)")
            .expect("fbm2 included");
        assert!(hash_at < snoise_at && snoise_at < fbm_at);
    }

    #[test]
    fn prelude_empty_when_no_builtins_referenced() {
        assert!(assemble_prelude("vec4 render(vec2 p) { return vec4(0.0); }").is_empty());
    }

    #[test]
    fn prototypes_cover_out_of_order_functions() {
        let authored = r#"
layout(binding = 0) uniform vec2 outputSize;
vec4 render(vec2 pos) { return helper(pos, 1.0); }
vec4 helper(vec2 scaledCoord, float time) { return vec4(scaledCoord, time, 1.0); }
"#;
        let protos = authored_prototypes(authored);
        assert!(protos.contains("vec4 render(vec2 pos);"));
        assert!(protos.contains("vec4 helper(vec2 scaledCoord, float time);"));
    }

    #[test]
    fn prototypes_skip_declarations_structs_and_initializers() {
        let authored = r#"
layout(binding = 0) uniform vec2 outputSize; // uniform
struct Cell { vec2 center; };
const vec3 COLORS[2] = vec3[2](vec3(0.0), vec3(1.0));
// vec4 commented(vec2 p) { }
float f(float x) { return x; }
"#;
        let protos = authored_prototypes(authored);
        assert_eq!(protos, "float f(float x);\n");
    }

    #[test]
    fn prototypes_are_emitted_callee_first() {
        // f1 calls f3, f3 calls f2; naga assigns arena slots at first
        // declaration, so prototypes must come out callee-first.
        let authored = "float f1(float x) { return f3(x); }\n\
                        float f2(float x) { return x; }\n\
                        float f3(float x) { return f2(x); }\n";
        assert_eq!(
            authored_prototypes(authored),
            "float f2(float x);\nfloat f3(float x);\nfloat f1(float x);\n"
        );
    }

    #[test]
    fn prototypes_normalize_multi_line_signatures() {
        let authored = "vec4 render(vec2 pos,\n            float t)\n{ return vec4(0.0); }";
        assert_eq!(
            authored_prototypes(authored),
            "vec4 render(vec2 pos, float t);\n"
        );
    }

    #[test]
    fn assembled_unit_has_version_prelude_prototypes_and_wrapper() {
        let authored = "layout(binding = 0) uniform vec2 outputSize;\n\
                        vec4 render(vec2 pos) { return vec4(lpfn_saturate(pos.x)); }\n";
        let unit =
            assemble_fragment_glsl(authored, &TextureBindingSpecs::new()).expect("assembles");
        assert!(unit.starts_with("#version 450 core\n"));
        let saturate_at = unit.find("float lpfn_saturate(").expect("prelude");
        let proto_at = unit.find("vec4 render(vec2 pos);").expect("prototype");
        let authored_at = unit.find("vec4 render(vec2 pos) {").expect("authored");
        let main_at = unit.find("void main()").expect("wrapper");
        assert!(saturate_at < proto_at && proto_at < authored_at && authored_at < main_at);
        assert!(unit.contains("render(floor(gl_FragCoord.xy))"));
    }
}
