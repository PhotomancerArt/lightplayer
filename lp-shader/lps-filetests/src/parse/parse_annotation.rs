//! Parse `// @kind(selector)` disposition lines.
//!
//! One vocabulary covers all four kinds (`unimplemented`, `unsupported`,
//! `broken`, `ignore`) and all three selector forms:
//!
//! ```text
//! // @unsupported(*)                          every target
//! // @broken(wasm.q32)                        one target, by canonical name
//! // @unimplemented(float_mode=f32)           an axis family
//! // @unsupported(frontend!=lp, backend!=wgpu)  a conjunction
//! ```
//!
//! An unknown axis key or an unknown axis value is a **hard error with a line
//! number**, never a selector that quietly matches nothing: a typo that matched
//! nothing would silently turn a disposition into a surprise red months later.

use crate::targets::{Annotation, AnnotationKind, Axis, AxisPredicate, Target, TargetSelector};
use anyhow::{Result, anyhow};

/// Try to parse an annotation from a comment line.
///
/// Returns `Ok(None)` when the line is not an annotation at all. `has_reason`
/// says whether a plain comment line precedes this one in the same comment
/// block; `@broken` requires one (see [`parse_selector`]'s sibling rule below).
pub fn parse_annotation_line(
    line: &str,
    line_number: usize,
    has_reason: bool,
) -> Result<Option<Annotation>> {
    let trimmed = line.trim();
    let rest = match trimmed.strip_prefix("// @") {
        Some(r) => r,
        None => return Ok(None),
    };

    let paren_start = rest
        .find('(')
        .ok_or_else(|| anyhow!("line {line_number}: annotation missing '('"))?;
    let kind_str = &rest[..paren_start];
    let kind = parse_annotation_kind(kind_str, line_number)?;

    let paren_end = rest
        .rfind(')')
        .ok_or_else(|| anyhow!("line {line_number}: annotation missing ')'"))?;
    let inner = rest[paren_start + 1..paren_end].trim();
    if inner.is_empty() {
        return Err(anyhow!(
            "line {line_number}: annotation requires a selector, expected a target name (e.g. wasm.q32), \
             an axis predicate (e.g. float_mode=f32), or *"
        ));
    }

    let selector = parse_selector(inner, line_number)?;

    if kind == AnnotationKind::Broken && !has_reason {
        return Err(anyhow!(
            "line {line_number}: @broken({selector}) has no reason, expected a comment line \
             immediately above it saying why (an unexplained @broken cannot be told apart from an \
             abandoned one)"
        ));
    }

    Ok(Some(Annotation {
        kind,
        selector,
        line_number,
    }))
}

/// Parse the text inside the parentheses.
pub fn parse_selector(inner: &str, line_number: usize) -> Result<TargetSelector> {
    if inner == "*" {
        return Ok(TargetSelector::All);
    }

    // A predicate is recognised by `=`; anything else must be a target name.
    if !inner.contains('=') {
        Target::from_name(inner).map_err(|e| anyhow!("line {line_number}: {e}"))?;
        return Ok(TargetSelector::Name(inner.to_string()));
    }

    let mut terms: Vec<AxisPredicate> = Vec::new();
    for raw in inner.split(',') {
        let term = raw.trim();
        if term.is_empty() {
            return Err(anyhow!(
                "line {line_number}: empty term in selector '{inner}', expected axis=value or axis!=value"
            ));
        }
        terms.push(parse_axis_predicate(term, line_number)?);
    }

    // An axis may be excluded several times (`backend!=interp, backend!=wgpu`
    // is how you subtract two targets from a family), but it may not be
    // constrained in a way no target can satisfy.
    for (i, term) in terms.iter().enumerate() {
        for prev in &terms[..i] {
            if prev.axis() != term.axis() {
                continue;
            }
            let unsatisfiable = match (prev.negated, term.negated) {
                (false, false) => prev.value != term.value,
                (false, true) | (true, false) => prev.value == term.value,
                (true, true) => false,
            };
            if unsatisfiable {
                return Err(anyhow!(
                    "line {line_number}: selector '{inner}' can never match — '{prev}' and \
                     '{term}' contradict"
                ));
            }
            if prev == term {
                return Err(anyhow!(
                    "line {line_number}: selector '{inner}' repeats '{term}'"
                ));
            }
        }
    }

    Ok(TargetSelector::Predicate(terms))
}

/// Parse one `axis=value` / `axis!=value` term.
fn parse_axis_predicate(term: &str, line_number: usize) -> Result<AxisPredicate> {
    let (key, value_str, negated) = match term.split_once("!=") {
        Some((k, v)) => (k.trim(), v.trim(), true),
        None => match term.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim(), false),
            None => {
                return Err(anyhow!(
                    "line {line_number}: '{term}' is not a predicate, expected axis=value or axis!=value \
                     with axis one of {}",
                    Axis::key_list()
                ));
            }
        },
    };

    let axis = Axis::from_key(key).ok_or_else(|| {
        anyhow!(
            "line {line_number}: unknown selector axis '{key}', expected one of {}",
            Axis::key_list()
        )
    })?;

    let value = axis.value_from_str(value_str).ok_or_else(|| {
        anyhow!(
            "line {line_number}: unknown value '{value_str}' for axis '{key}', expected one of {}",
            axis.value_list()
        )
    })?;

    Ok(AxisPredicate { value, negated })
}

fn parse_annotation_kind(s: &str, line_number: usize) -> Result<AnnotationKind> {
    match s.trim() {
        "unimplemented" => Ok(AnnotationKind::Unimplemented),
        "unsupported" => Ok(AnnotationKind::Unsupported),
        "broken" => Ok(AnnotationKind::Broken),
        "ignore" => Ok(AnnotationKind::Ignore),
        other => Err(anyhow!(
            "line {line_number}: invalid annotation kind '{other}', expected unimplemented, unsupported, broken, or ignore"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Option<Annotation>> {
        parse_annotation_line(line, 1, true)
    }

    #[test]
    fn test_parse_unimplemented_target() {
        let ann = parse("// @unimplemented(wasm.q32)").unwrap().unwrap();
        assert!(matches!(ann.kind, AnnotationKind::Unimplemented));
        assert_eq!(ann.selector, TargetSelector::Name("wasm.q32".to_string()));
    }

    #[test]
    fn test_parse_unsupported_target() {
        let ann = parse_annotation_line("// @unsupported(rv32c.q32)", 2, false)
            .unwrap()
            .unwrap();
        assert!(matches!(ann.kind, AnnotationKind::Unsupported));
        assert_eq!(ann.selector, TargetSelector::Name("rv32c.q32".to_string()));
        assert_eq!(ann.line_number, 2);
    }

    #[test]
    fn test_parse_broken_target() {
        let ann = parse("// @broken(wasm.q32)").unwrap().unwrap();
        assert!(matches!(ann.kind, AnnotationKind::Broken));
        assert_eq!(ann.selector, TargetSelector::Name("wasm.q32".to_string()));
    }

    #[test]
    fn test_parse_star() {
        let ann = parse("// @unsupported(*)").unwrap().unwrap();
        assert_eq!(ann.selector, TargetSelector::All);
    }

    #[test]
    fn test_parse_single_axis_predicate() {
        let ann = parse("// @unimplemented(float_mode=f32)").unwrap().unwrap();
        assert_eq!(ann.selector.to_string(), "float_mode=f32");
        assert_eq!(ann.selector.specificity(), 1);
    }

    #[test]
    fn test_parse_negated_and_conjunction() {
        let ann = parse("// @unsupported(frontend!=lp, backend!=wgpu)")
            .unwrap()
            .unwrap();
        assert_eq!(ann.selector.to_string(), "frontend!=lp, backend!=wgpu");
        assert_eq!(ann.selector.specificity(), 2);
    }

    #[test]
    fn test_parse_ignore_kind() {
        let ann = parse("// @ignore(float_mode=f32)").unwrap().unwrap();
        assert!(matches!(ann.kind, AnnotationKind::Ignore));
    }

    #[test]
    fn test_parse_every_axis_key_is_accepted() {
        for line in [
            "// @unsupported(frontend=naga)",
            "// @unsupported(backend=interp)",
            "// @unsupported(float_mode=q32)",
            "// @unsupported(isa=xtensa)",
            "// @unsupported(exec_mode=gpu)",
        ] {
            assert!(parse(line).unwrap().is_some(), "{line}");
        }
    }

    #[test]
    fn test_unknown_axis_key_errors_with_line_number() {
        let err = parse_annotation_line("// @unsupported(cpu=arm)", 42, true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 42"), "{err}");
        assert!(err.contains("unknown selector axis 'cpu'"), "{err}");
        assert!(err.contains("float_mode"), "error lists valid axes: {err}");
    }

    #[test]
    fn test_unknown_axis_value_errors_with_line_number() {
        let err = parse_annotation_line("// @unsupported(float_mode=f64)", 7, true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 7"), "{err}");
        assert!(err.contains("unknown value 'f64'"), "{err}");
        assert!(err.contains("q32, f32"), "error lists valid values: {err}");
    }

    #[test]
    fn test_typo_in_value_is_not_a_silent_no_match() {
        // `wgpu` is a backend, not a float mode. Matching nothing silently is
        // exactly the rot this rule exists to stop.
        assert!(parse("// @unsupported(float_mode=wgpu)").is_err());
    }

    /// Subtracting several targets from a family needs the same axis twice.
    #[test]
    fn test_repeated_negated_axis_is_allowed() {
        let ann = parse("// @unimplemented(float_mode=f32, backend!=interp, backend!=wgpu)")
            .unwrap()
            .unwrap();
        assert_eq!(ann.selector.specificity(), 3);
        let matched: Vec<String> = crate::targets::ALL_TARGETS
            .iter()
            .filter(|t| ann.selector.matches(t))
            .map(|t| t.name())
            .collect();
        // Both exclusions bite: `interp.f32` and `wgpu.f32` are subtracted from
        // the f32 family, leaving the compiled f32 targets. This assertion used
        // to expect nothing at all — the compiled f32 targets are the case the
        // selector was written to anticipate, and each new backend that gains an
        // f32 mode joins the list here.
        assert_eq!(
            matched,
            vec![
                "wasm.f32".to_string(),
                "rv32n.f32".to_string(),
                "rv32lpn.f32".to_string()
            ],
            "{matched:?}"
        );
    }

    #[test]
    fn test_contradictory_terms_error() {
        let err = parse("// @unsupported(backend=wasm, backend=interp)")
            .unwrap_err()
            .to_string();
        assert!(err.contains("can never match"), "{err}");

        let err = parse("// @unsupported(backend=wasm, backend!=wasm)")
            .unwrap_err()
            .to_string();
        assert!(err.contains("can never match"), "{err}");
    }

    #[test]
    fn test_repeated_identical_term_errors() {
        let err = parse("// @unsupported(backend!=wasm, backend!=wasm)")
            .unwrap_err()
            .to_string();
        assert!(err.contains("repeats"), "{err}");
    }

    #[test]
    fn test_empty_term_errors() {
        assert!(parse("// @unsupported(float_mode=f32,)").is_err());
    }

    #[test]
    fn test_broken_without_reason_errors() {
        let err = parse_annotation_line("// @broken(wasm.q32)", 9, false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("line 9"), "{err}");
        assert!(err.contains("no reason"), "{err}");
    }

    #[test]
    fn test_other_kinds_do_not_require_a_reason() {
        for line in [
            "// @unimplemented(wasm.q32)",
            "// @unsupported(*)",
            "// @ignore(float_mode=f32)",
        ] {
            assert!(
                parse_annotation_line(line, 1, false).unwrap().is_some(),
                "{line}"
            );
        }
    }

    #[test]
    fn test_parse_empty_parens_errors() {
        assert!(parse("// @unimplemented()").is_err());
    }

    #[test]
    fn test_parse_invalid_target_errors() {
        assert!(parse("// @unimplemented(nope)").is_err());
    }

    #[test]
    fn test_parse_not_annotation() {
        assert!(parse("// run: test() == 1").unwrap().is_none());
    }

    #[test]
    fn test_parse_invalid_kind() {
        let err = parse("// @foobar(wasm.q32)").unwrap_err().to_string();
        assert!(err.contains("invalid annotation kind"), "{err}");
        assert!(err.contains("ignore"), "{err}");
    }
}
