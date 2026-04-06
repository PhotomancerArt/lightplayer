//! Parse `@unimplemented(target)` and `@unsupported(target)` comment lines.

use crate::targets::{Annotation, AnnotationKind, Target};
use anyhow::{Result, anyhow};

/// Try to parse an annotation from a comment line.
/// Returns None if the line is not an annotation.
pub fn parse_annotation_line(line: &str, line_number: usize) -> Result<Option<Annotation>> {
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
            "line {line_number}: annotation requires an explicit target (e.g. wasm.q32)"
        ));
    }

    Target::from_name(inner).map_err(|e| anyhow!("line {line_number}: {e}"))?;

    Ok(Some(Annotation {
        kind,
        target: inner.to_string(),
        line_number,
    }))
}

fn parse_annotation_kind(s: &str, line_number: usize) -> Result<AnnotationKind> {
    match s.trim() {
        "unimplemented" => Ok(AnnotationKind::Unimplemented),
        "unsupported" => Ok(AnnotationKind::Unsupported),
        other => Err(anyhow!(
            "line {line_number}: invalid annotation kind '{other}', expected unimplemented or unsupported"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unimplemented_target() {
        let ann = parse_annotation_line("// @unimplemented(wasm.q32)", 1)
            .unwrap()
            .unwrap();
        assert!(matches!(ann.kind, AnnotationKind::Unimplemented));
        assert_eq!(ann.target, "wasm.q32");
    }

    #[test]
    fn test_parse_unsupported_target() {
        let ann = parse_annotation_line("// @unsupported(rv32.q32)", 2)
            .unwrap()
            .unwrap();
        assert!(matches!(ann.kind, AnnotationKind::Unsupported));
        assert_eq!(ann.target, "rv32.q32");
    }

    #[test]
    fn test_parse_empty_parens_errors() {
        assert!(parse_annotation_line("// @unimplemented()", 1).is_err());
    }

    #[test]
    fn test_parse_invalid_target_errors() {
        assert!(parse_annotation_line("// @unimplemented(nope)", 1).is_err());
    }

    #[test]
    fn test_parse_not_annotation() {
        assert!(
            parse_annotation_line("// run: test() == 1", 1)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_parse_invalid_kind() {
        assert!(parse_annotation_line("// @foobar(wasm.q32)", 1).is_err());
    }
}
