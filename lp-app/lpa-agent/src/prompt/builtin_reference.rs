//! Builtin function reference for the system prompt, generated from
//! `lps_builtins::canonical_glsl::CANONICAL_GLSL`.
//!
//! Entries are grouped by their path segment (color/generative/math/...);
//! function signatures are extracted from each source with a best-effort
//! string scan (falling back to the entry name when nothing scans).

use std::collections::BTreeMap;

use lps_builtins::canonical_glsl::CANONICAL_GLSL;

/// GLSL return-type tokens a signature line can start with.
const GLSL_TYPES: &[&str] = &[
    "float", "vec2", "vec3", "vec4", "int", "uint", "ivec2", "ivec3", "ivec4", "uvec2", "uvec3",
    "uvec4", "bool", "bvec2", "bvec3", "bvec4", "void",
];

/// Render the grouped builtin reference (markdown-ish plain text).
pub fn builtin_reference() -> String {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in CANONICAL_GLSL {
        let group = group_of(entry.path);
        let mut signatures = scan_signatures(entry.source);
        if signatures.is_empty() {
            signatures.push(format!("{} (see builtin docs)", entry.name));
        }
        groups.entry(group).or_default().extend(signatures);
    }

    let mut out = String::new();
    for (group, signatures) in &groups {
        out.push_str(group);
        out.push_str(":\n");
        for sig in signatures {
            out.push_str("  ");
            out.push_str(sig);
            out.push('\n');
        }
    }
    out
}

/// `glsl/lpfn/color/space/hsv2rgb.glsl` → `color/space`; top-level files →
/// `core`.
fn group_of(path: &str) -> String {
    let rel = path.strip_prefix("glsl/lpfn/").unwrap_or(path);
    match rel.rsplit_once('/') {
        Some((dir, _file)) => dir.to_string(),
        None => "core".to_string(),
    }
}

/// Best-effort scan for `lpfn_*` function signature lines: a GLSL type
/// token, an `lpfn_` identifier, and a parenthesized parameter list.
fn scan_signatures(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some((ty, rest)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        if !GLSL_TYPES.contains(&ty) {
            continue;
        }
        let rest = rest.trim_start();
        if !rest.starts_with("lpfn_") {
            continue;
        }
        let Some(close) = rest.find(')') else {
            continue;
        };
        if !rest[..close].contains('(') {
            continue;
        }
        out.push(format!("{ty} {}", &rest[..=close]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_covers_known_builtins_grouped_by_path() {
        let reference = builtin_reference();
        assert!(reference.contains("color/space:"), "{reference}");
        assert!(
            reference.contains("vec3 lpfn_hsv2rgb(vec3 hsv)"),
            "{reference}"
        );
        assert!(reference.contains("core:"), "{reference}");
        assert!(reference.contains("uint lpfn_hash("), "{reference}");
        // Every canonical entry contributes at least one line to its group.
        for entry in CANONICAL_GLSL {
            let sigs = scan_signatures(entry.source);
            assert!(
                !sigs.is_empty() || reference.contains(entry.name),
                "entry {} missing from reference",
                entry.name
            );
        }
    }

    #[test]
    fn signature_scan_extracts_overloads() {
        let sigs = scan_signatures(
            "// comment\nvec3 lpfn_x(vec3 a) {\n  return a;\n}\nvec4 lpfn_x(vec4 a) { return a; }\nfloat helper(float y) { return y; }\n",
        );
        assert_eq!(sigs, ["vec3 lpfn_x(vec3 a)", "vec4 lpfn_x(vec4 a)"]);
    }
}
