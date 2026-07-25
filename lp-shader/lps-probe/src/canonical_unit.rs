//! Canonical-builtin compilation-unit assembly.
//!
//! `lps-frontend` reserves the `lpfn_` name prefix: every call to an `lpfn_*`
//! function is lowered to an `@lpfn::` builtin import, which the LPIR
//! interpreter cannot evaluate. When a source references `lpfn_*`, the
//! canonical GLSL builtin sources ([`lps_builtins::canonical_glsl`]) are
//! prepended and the whole unit gets an `lpfn_` → `lpo_` identifier rename,
//! so the canonical bodies compile as ordinary local functions with f32
//! semantics (same dance as the `lps-filetests` conformance oracle, from
//! which these helpers were extracted).

use alloc::string::{String, ToString};

use lps_builtins::canonical_glsl::CANONICAL_GLSL;

/// True if `src` references an `lpfn_*` identifier (at identifier boundary).
pub fn references_lpfn(src: &str) -> bool {
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(pos) = src[from..].find("lpfn_") {
        let i = from + pos;
        let at_boundary = i == 0 || !is_ident_byte(bytes[i - 1]);
        if at_boundary {
            return true;
        }
        from = i + 1;
    }
    false
}

/// Assemble the canonical compilation unit: every canonical source plus
/// `snippet`, with the `lpfn_` → `lpo_` rename applied throughout so the
/// canonical bodies compile as ordinary local GLSL functions.
pub fn canonical_unit_source(snippet: &str) -> String {
    let mut src = String::new();
    // CANONICAL_GLSL is dependency-ordered (asserted by its unit tests),
    // so plain concatenation satisfies GLSL declaration-before-use.
    for c in CANONICAL_GLSL {
        src.push_str(&rename_lpfn_prefix(c.source));
        src.push('\n');
    }
    src.push_str(&rename_lpfn_prefix(snippet));
    src
}

/// Compose the execution unit for `snippet`: the canonical unit when the
/// snippet references `lpfn_*`, the snippet itself otherwise.
pub fn execution_unit_source(snippet: &str) -> String {
    if references_lpfn(snippet) {
        canonical_unit_source(snippet)
    } else {
        snippet.to_string()
    }
}

/// Rename the `lpfn_` identifier prefix to `lpo_` at identifier boundaries.
pub fn rename_lpfn_prefix(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let at_boundary = i == 0 || !is_ident_byte(bytes[i - 1]);
        if at_boundary && src[i..].starts_with("lpfn_") {
            out.push_str("lpo_");
            i += "lpfn_".len();
        } else {
            // Advance one UTF-8 scalar (sources are ASCII; be safe anyway).
            let ch = src[i..].chars().next().expect("in-bounds char");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_lpfn_boundary() {
        assert!(references_lpfn("float f() { return lpfn_saturate(2.0); }"));
        assert!(!references_lpfn("float my_lpfn_thing() { return 1.0; }"));
        assert!(!references_lpfn("float f() { return 1.0; }"));
    }

    #[test]
    fn rename_respects_identifier_boundaries() {
        assert_eq!(rename_lpfn_prefix("lpfn_hash(x)"), "lpo_hash(x)");
        assert_eq!(rename_lpfn_prefix("my_lpfn_hash"), "my_lpfn_hash");
        assert_eq!(rename_lpfn_prefix("a lpfn_a(lpfn_b)"), "a lpo_a(lpo_b)");
    }

    #[test]
    fn execution_unit_passes_plain_sources_through() {
        let src = "float f() { return 1.0; }";
        assert_eq!(execution_unit_source(src), src);
        let with_lpfn = "float f() { return lpfn_saturate(2.0); }";
        let unit = execution_unit_source(with_lpfn);
        assert!(unit.contains("lpo_saturate(2.0)"));
        assert!(unit.len() > with_lpfn.len());
    }
}
