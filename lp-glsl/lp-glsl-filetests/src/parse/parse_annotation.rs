//! Parse @unimplemented, @broken, @ignore annotation lines.

use crate::target::{Annotation, AnnotationKind, Backend, ExecMode, FloatMode, Isa, TargetFilter};
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
    let params_str = rest[paren_start + 1..paren_end].trim();

    let (filter, reason) = parse_params(params_str, line_number)?;

    Ok(Some(Annotation {
        kind,
        filter,
        reason,
        line_number,
    }))
}

fn parse_annotation_kind(s: &str, line_number: usize) -> Result<AnnotationKind> {
    match s.trim() {
        "unimplemented" => Ok(AnnotationKind::Unimplemented),
        "broken" => Ok(AnnotationKind::Broken),
        "ignore" => Ok(AnnotationKind::Ignore),
        other => Err(anyhow!(
            "line {line_number}: invalid annotation kind '{other}', expected unimplemented, broken, or ignore"
        )),
    }
}

fn parse_params(s: &str, line_number: usize) -> Result<(TargetFilter, Option<String>)> {
    let mut filter = TargetFilter::default();
    let mut reason = None;

    if s.is_empty() {
        return Ok((filter, reason));
    }

    for param in split_params(s) {
        let param = param.trim();
        if param.is_empty() {
            continue;
        }
        let eq_pos = param
            .find('=')
            .ok_or_else(|| anyhow!("line {line_number}: expected key=value in param '{param}'"))?;
        let key = param[..eq_pos].trim();
        let value_str = param[eq_pos + 1..].trim();

        let value = if value_str.starts_with('"') {
            if !value_str.ends_with('"') || value_str.len() < 2 {
                anyhow::bail!("line {line_number}: unclosed quoted string in '{value_str}'");
            }
            value_str[1..value_str.len() - 1].to_string()
        } else {
            value_str.to_string()
        };

        match key {
            "backend" => filter.backend = Some(parse_backend(&value, line_number)?),
            "float_mode" => filter.float_mode = Some(parse_float_mode(&value, line_number)?),
            "isa" => filter.isa = Some(parse_isa(&value, line_number)?),
            "exec_mode" => filter.exec_mode = Some(parse_exec_mode(&value, line_number)?),
            "reason" => reason = Some(value),
            other => {
                return Err(anyhow!(
                    "line {line_number}: invalid annotation key '{other}', expected backend, float_mode, isa, exec_mode, or reason"
                ));
            }
        }
    }

    Ok((filter, reason))
}

/// Split params by comma, respecting quoted strings.
fn split_params(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    let bytes = s.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' {
            in_quote = !in_quote;
        } else if !in_quote && b == b',' {
            result.push(s[start..i].trim());
            start = i + 1;
        }
    }
    result.push(s[start..].trim());
    result
}

fn parse_backend(s: &str, line_number: usize) -> Result<Backend> {
    match s {
        "cranelift" => Ok(Backend::Cranelift),
        "wasm" => Ok(Backend::Wasm),
        other => Err(anyhow!(
            "line {line_number}: invalid backend '{other}', expected cranelift or wasm"
        )),
    }
}

fn parse_float_mode(s: &str, line_number: usize) -> Result<FloatMode> {
    match s {
        "q32" => Ok(FloatMode::Q32),
        "f32" => Ok(FloatMode::F32),
        other => Err(anyhow!(
            "line {line_number}: invalid float_mode '{other}', expected q32 or f32"
        )),
    }
}

fn parse_isa(s: &str, line_number: usize) -> Result<Isa> {
    match s {
        "riscv32" => Ok(Isa::Riscv32),
        "wasm32" => Ok(Isa::Wasm32),
        "native" => Ok(Isa::Native),
        other => Err(anyhow!(
            "line {line_number}: invalid isa '{other}', expected riscv32, wasm32, or native"
        )),
    }
}

fn parse_exec_mode(s: &str, line_number: usize) -> Result<ExecMode> {
    match s {
        "jit" => Ok(ExecMode::Jit),
        "emulator" => Ok(ExecMode::Emulator),
        other => Err(anyhow!(
            "line {line_number}: invalid exec_mode '{other}', expected jit or emulator"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_annotation() {
        let ann = parse_annotation_line("// @unimplemented()", 1)
            .unwrap()
            .unwrap();
        assert!(matches!(ann.kind, AnnotationKind::Unimplemented));
        assert!(ann.filter.backend.is_none());
        assert!(ann.reason.is_none());
    }

    #[test]
    fn test_parse_backend_filter() {
        let ann = parse_annotation_line("// @unimplemented(backend=wasm)", 1)
            .unwrap()
            .unwrap();
        assert_eq!(ann.filter.backend, Some(Backend::Wasm));
    }

    #[test]
    fn test_parse_multiple_filters() {
        let ann = parse_annotation_line("// @broken(backend=cranelift, isa=riscv32)", 1)
            .unwrap()
            .unwrap();
        assert_eq!(ann.filter.backend, Some(Backend::Cranelift));
        assert_eq!(ann.filter.isa, Some(Isa::Riscv32));
    }

    #[test]
    fn test_parse_with_reason() {
        let ann = parse_annotation_line("// @broken(isa=riscv32, reason=\"overflow\")", 1)
            .unwrap()
            .unwrap();
        assert_eq!(ann.filter.isa, Some(Isa::Riscv32));
        assert_eq!(ann.reason, Some("overflow".to_string()));
    }

    #[test]
    fn test_parse_ignore() {
        let ann = parse_annotation_line("// @ignore(backend=wasm)", 1)
            .unwrap()
            .unwrap();
        assert!(matches!(ann.kind, AnnotationKind::Ignore));
    }

    #[test]
    fn test_parse_all_filter_fields() {
        let ann = parse_annotation_line(
            "// @ignore(backend=cranelift, float_mode=q32, isa=riscv32, exec_mode=emulator)",
            1,
        )
        .unwrap()
        .unwrap();
        assert_eq!(ann.filter.backend, Some(Backend::Cranelift));
        assert_eq!(ann.filter.float_mode, Some(FloatMode::Q32));
        assert_eq!(ann.filter.isa, Some(Isa::Riscv32));
        assert_eq!(ann.filter.exec_mode, Some(ExecMode::Emulator));
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
    fn test_parse_not_comment() {
        assert!(parse_annotation_line("int x = 5;", 1).unwrap().is_none());
    }

    #[test]
    fn test_parse_invalid_kind() {
        assert!(parse_annotation_line("// @foobar()", 1).is_err());
    }

    #[test]
    fn test_parse_invalid_key() {
        assert!(parse_annotation_line("// @broken(foo=bar)", 1).is_err());
    }

    #[test]
    fn test_parse_invalid_backend() {
        assert!(parse_annotation_line("// @broken(backend=gcc)", 1).is_err());
    }

    #[test]
    fn test_parse_reason_with_quotes() {
        let ann = parse_annotation_line("// @broken(reason=\"has spaces and, commas\")", 1)
            .unwrap()
            .unwrap();
        assert_eq!(ann.reason, Some("has spaces and, commas".to_string()));
    }

    #[test]
    fn test_parse_whitespace_tolerance() {
        let ann =
            parse_annotation_line("// @unimplemented( backend = wasm , float_mode = q32 )", 1)
                .unwrap()
                .unwrap();
        assert_eq!(ann.filter.backend, Some(Backend::Wasm));
        assert_eq!(ann.filter.float_mode, Some(FloatMode::Q32));
    }
}
