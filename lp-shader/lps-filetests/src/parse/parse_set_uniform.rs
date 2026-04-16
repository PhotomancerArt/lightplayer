//! Parse `// set_uniform: name = value` directives.

use anyhow::{Context, Result};

use super::test_type::SetUniform;

/// Returns the remainder after `// set_uniform:` if this line is a set-uniform directive.
pub fn parse_set_uniform_line(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    trimmed.strip_prefix("// set_uniform:").map(|s| s.trim())
}

/// Parse `name = value` from the line body (after the `// set_uniform:` prefix).
pub fn parse_set_uniform_body(body: &str, line_number: usize) -> Result<SetUniform> {
    let body = body.trim();
    let eq_pos = body
        .find(" = ")
        .with_context(|| format!("set_uniform at line {line_number}: expected ` = `"))?;
    let name = body[..eq_pos].trim().to_string();
    let value_str = body[eq_pos + 3..].trim().to_string();
    if name.is_empty() {
        anyhow::bail!("set_uniform at line {line_number}: empty name");
    }
    if value_str.is_empty() {
        anyhow::bail!("set_uniform at line {line_number}: empty value");
    }
    Ok(SetUniform { name, value_str })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_detection() {
        assert_eq!(
            parse_set_uniform_line("// set_uniform: u_time = 3.0"),
            Some("u_time = 3.0")
        );
        assert_eq!(parse_set_uniform_line("// run: x() == 1"), None);
    }

    #[test]
    fn parse_body() {
        let u = parse_set_uniform_body("u_resolution = vec2(1.0, 2.0)", 1).unwrap();
        assert_eq!(u.name, "u_resolution");
        assert_eq!(u.value_str, "vec2(1.0, 2.0)");
    }
}
