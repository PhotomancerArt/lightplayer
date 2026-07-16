//! Drift tests: tie the generated completion manifest to `BuiltinId::all()`,
//! the generated name -> `BuiltinId` mapping tables, and the compiler's
//! inlined-builtin inventory (`lps_glsl::builtin_inventory`).
//!
//! These fail when a builtin is added/removed/renamed without regenerating
//! (`cargo run -p lps-builtins-gen-app`), when the manifest contains a phantom
//! name the compiler does not accept, or when a builtin has no completion.

use std::collections::HashSet;

use lps_builtin_completions::{COMPLETIONS, CompletionEntry};
use lps_builtin_ids::{
    BuiltinId, GlslParamKind, Mode, Module, glsl_lpfn_q32_builtin_id, glsl_q32_math_builtin_id,
    texture_q32_builtin_id,
};
use lps_glsl::builtin_inventory::INLINED_BUILTINS;

/// Argument count encoded in a snippet ("f(${a}, ${b})" has two placeholders).
fn snippet_arity(entry: &CompletionEntry) -> usize {
    entry.snippet.matches("${").count()
}

/// Parse an LPFN detail string ("float lpfn_fbm(vec2 p, int octaves, uint seed)")
/// into the GLSL-facing name and overload parameter kinds.
fn parse_lpfn_detail(detail: &str) -> (String, Vec<GlslParamKind>) {
    let open = detail
        .find('(')
        .unwrap_or_else(|| panic!("no '(' in detail {detail:?}"));
    let close = detail
        .rfind(')')
        .unwrap_or_else(|| panic!("no ')' in detail {detail:?}"));
    let name = detail[..open]
        .split_whitespace()
        .last()
        .unwrap_or_else(|| panic!("no name in detail {detail:?}"))
        .to_string();
    let params = detail[open + 1..close].trim();
    let mut kinds = Vec::new();
    if !params.is_empty() {
        for param in params.split(',') {
            let mut tokens = param.split_whitespace();
            let mut ty = tokens
                .next()
                .unwrap_or_else(|| panic!("empty param in detail {detail:?}"));
            if matches!(ty, "in" | "out" | "inout") {
                ty = tokens
                    .next()
                    .unwrap_or_else(|| panic!("qualifier without type in {detail:?}"));
            }
            kinds.push(match ty {
                "bool" => GlslParamKind::Bool,
                "int" => GlslParamKind::Int,
                "uint" => GlslParamKind::UInt,
                "float" => GlslParamKind::Float,
                "vec2" => GlslParamKind::Vec2,
                "vec3" => GlslParamKind::Vec3,
                "vec4" => GlslParamKind::Vec4,
                "ivec2" => GlslParamKind::IVec2,
                "ivec3" => GlslParamKind::IVec3,
                "ivec4" => GlslParamKind::IVec4,
                "uvec2" => GlslParamKind::UVec2,
                "uvec3" => GlslParamKind::UVec3,
                "uvec4" => GlslParamKind::UVec4,
                "bvec2" => GlslParamKind::BVec2,
                "bvec3" => GlslParamKind::BVec3,
                "bvec4" => GlslParamKind::BVec4,
                other => panic!("unexpected GLSL param type {other:?} in detail {detail:?}"),
            });
        }
    }
    (name, kinds)
}

fn entries(module: &str) -> impl Iterator<Item = &'static CompletionEntry> + '_ {
    COMPLETIONS.iter().filter(move |e| e.module == module)
}

#[test]
fn manifest_is_sorted_and_unique() {
    let keys: Vec<(&str, &str, &str)> = COMPLETIONS
        .iter()
        .map(|e| (e.name, e.module, e.detail))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        keys, sorted,
        "COMPLETIONS must be sorted by (name, module, detail) and unique"
    );
    for entry in COMPLETIONS {
        assert!(
            matches!(entry.module, "lpfn" | "glsl" | "texture"),
            "module {:?} for {:?} is not user-callable GLSL \
             (lpir/vm internals must not be offered as completions)",
            entry.module,
            entry.name
        );
        assert!(
            !entry.name.starts_with("__"),
            "internal name {:?} offered as a completion",
            entry.name
        );
    }
}

#[test]
fn lpfn_entries_match_builtin_ids() {
    let mut covered: HashSet<BuiltinId> = HashSet::new();
    for entry in entries("lpfn") {
        assert!(
            entry.name.starts_with("lpfn_"),
            "lpfn entry with non-lpfn name {:?}",
            entry.name
        );
        let (name, kinds) = parse_lpfn_detail(entry.detail);
        assert_eq!(
            name, entry.name,
            "detail/name mismatch for {:?}",
            entry.detail
        );
        assert_eq!(
            snippet_arity(entry),
            kinds.len(),
            "snippet arity mismatch for {:?}",
            entry.detail
        );
        assert!(
            !entry.description.is_empty(),
            "LPFN entry {:?} has no description (doc comment missing?)",
            entry.detail
        );
        let id = glsl_lpfn_q32_builtin_id(&name, &kinds).unwrap_or_else(|| {
            panic!(
                "phantom LPFN completion (no BuiltinId overload): {:?}",
                entry.detail
            )
        });
        assert!(
            covered.insert(id),
            "duplicate LPFN completion for {id:?}: {:?}",
            entry.detail
        );
    }
    // Every LPFN builtin is reachable from a manifest entry. F32 twins share
    // their Q32 twin's signature (validated as decimal pairs), so checking the
    // Q32/variant-less ids covers the whole module.
    for id in BuiltinId::all() {
        if id.module() != Module::Lpfn || id.mode() == Some(Mode::F32) {
            continue;
        }
        assert!(
            covered.contains(id),
            "LPFN builtin {id:?} has no completion entry"
        );
    }
}

#[test]
fn glsl_entries_match_mapping_table_or_inlined_inventory() {
    let inlined: HashSet<(&str, usize)> = INLINED_BUILTINS
        .iter()
        .map(|b| (b.name, b.params.len()))
        .collect();
    let mut covered: HashSet<BuiltinId> = HashSet::new();
    let mut seen_keys: HashSet<(&str, usize)> = HashSet::new();
    for entry in entries("glsl") {
        let arity = snippet_arity(entry);
        let import_id = glsl_q32_math_builtin_id(entry.name, arity);
        assert!(
            import_id.is_some() || inlined.contains(&(entry.name, arity)),
            "phantom glsl completion (neither import table nor inlined \
             inventory): {}/{arity} args",
            entry.name
        );
        if let Some(id) = import_id {
            covered.insert(id);
        }
        assert!(
            seen_keys.insert((entry.name, arity)),
            "duplicate glsl completion for ({}, {arity})",
            entry.name
        );
    }
    for id in BuiltinId::all() {
        if id.module() != Module::Glsl {
            continue;
        }
        assert!(
            covered.contains(id),
            "glsl builtin {id:?} not reachable from any completion entry"
        );
    }
    // Every inlined builtin (mix/clamp/dot/…) has its completion.
    for (name, arity) in &inlined {
        assert!(
            seen_keys.contains(&(name, *arity)),
            "inlined builtin {name:?}/{arity} args has no completion entry"
        );
    }
}

#[test]
fn ir_and_vm_internals_are_absent() {
    assert_eq!(
        entries("lpir").count() + entries("vm").count(),
        0,
        "lpir/vm builtins are internals and must not be completions"
    );
}

#[test]
fn texture_entries_match_mapping_table() {
    let mut covered: HashSet<BuiltinId> = HashSet::new();
    for entry in entries("texture") {
        let arity = snippet_arity(entry);
        let id = texture_q32_builtin_id(entry.name, arity)
            .unwrap_or_else(|| panic!("phantom texture completion: {}/{arity} args", entry.name));
        assert!(
            covered.insert(id),
            "duplicate texture completion for {id:?}"
        );
    }
    for id in BuiltinId::all() {
        if id.module() != Module::Texture {
            continue;
        }
        assert!(
            covered.contains(id),
            "texture builtin {id:?} has no completion entry"
        );
    }
}
