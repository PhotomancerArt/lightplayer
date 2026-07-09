//! Assemble the lpfn GLSL prelude from the M2 canonical sources.
//!
//! Scans the authored shader for `lpfn_*` references and splices every
//! canonical source that *defines* a referenced name (all overloads), plus
//! transitive dependencies, in `CANONICAL_GLSL`'s dependency order. Driving
//! naga `glsl-in` directly (not through `lps-frontend`) means the `lpfn_`
//! prefix is not reserved and the prelude functions resolve as plain local
//! GLSL functions.

use std::collections::BTreeSet;

use lps_builtins::canonical_glsl::{CANONICAL_GLSL, CanonicalGlsl};

/// Build the prelude GLSL for an authored source: the concatenation of every
/// needed canonical source, dependency-ordered. Returns an empty string when
/// the shader references no lpfn builtins.
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

/// All `lpfn_*` identifiers referenced in a GLSL source (at identifier
/// boundaries; comments are not stripped — good enough for a spike, and
/// over-inclusion only pads the prelude).
pub fn lpfn_references(src: &str) -> BTreeSet<String> {
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

fn strip_keyword<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(keyword)?;
    // Must be followed by whitespace (a real declarator, not a prefix match).
    let rest = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('\t'))?;
    Some(rest.trim_start())
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_finds_lpfn_references_at_boundaries() {
        let refs = lpfn_references("x = lpfn_worley(p, 0u) + my_lpfn_fake(1); lpfn_hsv2rgb(v);");
        assert!(refs.contains("lpfn_worley"));
        assert!(refs.contains("lpfn_hsv2rgb"));
        assert!(!refs.contains("lpfn_fake"));
    }

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
}
