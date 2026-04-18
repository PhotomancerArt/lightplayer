//! Parse `// compile-opt(key, value)` file-level LPIR compiler options.

use anyhow::{Result, anyhow};

/// If `line` is a `// compile-opt(...)` directive, return `(key, value)` (trimmed).
/// Returns `Ok(None)` if the line is not a compile-opt directive.
pub fn parse_compile_opt_line(line: &str, line_number: usize) -> Result<Option<(String, String)>> {
    let trimmed = line.trim();
    let after_slash = match trimmed.strip_prefix("//") {
        Some(s) => s.trim_start(),
        None => return Ok(None),
    };
    let after_kw = match after_slash.strip_prefix("compile-opt") {
        Some(s) => s.trim_start(),
        None => return Ok(None),
    };
    if !after_kw.starts_with('(') {
        return Err(anyhow!(
            "line {line_number}: `compile-opt` must be followed by '('"
        ));
    }
    let inner = &after_kw[1..];
    let close = inner
        .rfind(')')
        .ok_or_else(|| anyhow!("line {line_number}: `compile-opt` missing ')'"))?;
    let inner = inner[..close].trim();
    if inner.is_empty() {
        return Err(anyhow!(
            "line {line_number}: `compile-opt` requires key and value inside parentheses"
        ));
    }
    let comma = inner.find(',').ok_or_else(|| {
        anyhow!("line {line_number}: `compile-opt` expects `key, value` (comma-separated)")
    })?;
    let key = inner[..comma].trim();
    let value = inner[comma + 1..].trim();
    if key.is_empty() || value.is_empty() {
        return Err(anyhow!(
            "line {line_number}: `compile-opt` key and value must be non-empty"
        ));
    }
    Ok(Some((key.to_string(), value.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic() {
        let r = parse_compile_opt_line("// compile-opt(inline.mode, never)", 1)
            .unwrap()
            .unwrap();
        assert_eq!(r.0, "inline.mode");
        assert_eq!(r.1, "never");
    }

    #[test]
    fn parses_whitespace() {
        let r = parse_compile_opt_line("  //   compile-opt(  inline.mode  ,  auto  )  ", 2)
            .unwrap()
            .unwrap();
        assert_eq!(r.0, "inline.mode");
        assert_eq!(r.1, "auto");
    }

    #[test]
    fn not_directive() {
        assert!(
            parse_compile_opt_line("// run: x() == 1", 1)
                .unwrap()
                .is_none()
        );
        assert!(
            parse_compile_opt_line("// @unimplemented(wasm.q32)", 1)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn missing_comma_errors() {
        assert!(parse_compile_opt_line("// compile-opt(inline.mode)", 1).is_err());
    }

    #[test]
    fn value_may_contain_commas() {
        let r = parse_compile_opt_line("// compile-opt(a, b, c)", 2)
            .unwrap()
            .unwrap();
        assert_eq!(r.0, "a");
        assert_eq!(r.1, "b, c");
    }
}
