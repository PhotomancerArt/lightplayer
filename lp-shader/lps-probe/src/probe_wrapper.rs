//! Probe wrapper emission and probe-attributed diagnostic remapping.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::probe_domain::ProbeDomain;
use crate::probe_spec::ProbeSpec;

/// An emitted single-line wrapper function for one probe.
pub struct ProbeWrapper {
    /// GLSL function name (`__probe_<id>`).
    pub fn_name: String,
    /// The wrapper source line (no trailing newline).
    pub text: String,
    /// Byte offset of the expression inside `text` (for column remapping).
    pub expr_offset: usize,
}

/// True if `id` is a valid probe id: `[a-z0-9_]+`.
pub fn probe_id_is_valid(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// True if `s` is a plausible GLSL identifier (`[A-Za-z_][A-Za-z0-9_]*`).
pub fn glsl_ident_is_valid(s: &str) -> bool {
    let mut bytes = s.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_alphabetic() || b == b'_' => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Emit the wrapper for `probe`. Position domains take `vec2 pos`;
/// [`ProbeDomain::Sweep`] takes `float <var>` instead.
pub fn emit_wrapper(probe: &ProbeSpec) -> ProbeWrapper {
    let fn_name = format!("__probe_{}", probe.id);
    let ty = probe.ty.glsl_name();
    let param = match &probe.domain {
        ProbeDomain::Sweep { var, .. } => format!("float {var}"),
        _ => String::from("vec2 pos"),
    };
    let head = format!("{ty} {fn_name}({param}) {{ return (");
    let expr_offset = head.len();
    let text = format!("{head}{}); }}", probe.expr);
    ProbeWrapper {
        fn_name,
        text,
        expr_offset,
    }
}

/// Remap probe-compile diagnostics so positions at/after the wrapper's first
/// line are attributed to the probe expression.
///
/// Two rewrites, both string-level (diagnostics arrive rendered):
/// - `glsl:<line>:<col>` markers with `line >= wrapper_first_line` become
///   `probe '<id>' expr:<line'>:<col'>` (line relative to the wrapper,
///   column shifted by the wrapper prelude on its first line).
/// - `in function '__probe_<id>'` (lowering errors) becomes
///   `in probe '<id>' expr`.
pub fn remap_probe_diagnostics(
    diags: Vec<String>,
    id: &str,
    wrapper_first_line: usize,
    expr_offset: usize,
) -> Vec<String> {
    diags
        .into_iter()
        .map(|d| remap_one(&d, id, wrapper_first_line, expr_offset))
        .collect()
}

fn remap_one(diag: &str, id: &str, wrapper_first_line: usize, expr_offset: usize) -> String {
    let mut out = String::with_capacity(diag.len());
    let mut rest = diag;
    while let Some(pos) = rest.find("glsl:") {
        let (before, tail) = rest.split_at(pos);
        out.push_str(before);
        match parse_line_col(&tail["glsl:".len()..]) {
            Some((line, col, consumed)) if line >= wrapper_first_line => {
                let line_rel = line - wrapper_first_line + 1;
                // Columns are 1-based; shift only on the wrapper's first
                // line, where the emitted prelude precedes the expression.
                let col_rel = if line_rel == 1 && col > expr_offset {
                    col - expr_offset
                } else {
                    col
                };
                out.push_str(&format!("probe '{id}' expr:{line_rel}:{col_rel}"));
                rest = &tail["glsl:".len() + consumed..];
            }
            _ => {
                out.push_str("glsl:");
                rest = &tail["glsl:".len()..];
            }
        }
    }
    out.push_str(rest);
    // Lowering errors name the wrapper function instead of a position.
    let in_fn = format!("in function '__probe_{id}'");
    out.replace(&in_fn, &format!("in probe '{id}' expr"))
}

/// Parse `<digits>:<digits>` returning (line, col, bytes consumed).
fn parse_line_col(s: &str) -> Option<(usize, usize, usize)> {
    let bytes = s.as_bytes();
    let line_end = bytes.iter().position(|b| !b.is_ascii_digit())?;
    if line_end == 0 || bytes.get(line_end) != Some(&b':') {
        return None;
    }
    let col_start = line_end + 1;
    let col_len = bytes[col_start..]
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(bytes.len() - col_start);
    if col_len == 0 {
        return None;
    }
    let line: usize = s[..line_end].parse().ok()?;
    let col: usize = s[col_start..col_start + col_len].parse().ok()?;
    Some((line, col, col_start + col_len))
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;
    use crate::probe_reduce::ProbeReduce;
    use crate::probe_spec::ProbeType;

    #[test]
    fn probe_ids_are_validated() {
        assert!(probe_id_is_valid("center_red"));
        assert!(probe_id_is_valid("p1"));
        assert!(!probe_id_is_valid(""));
        assert!(!probe_id_is_valid("Bad"));
        assert!(!probe_id_is_valid("a-b"));
    }

    #[test]
    fn wrapper_shapes() {
        let pos_probe = ProbeSpec {
            id: "p".to_string(),
            ty: ProbeType::Vec4,
            expr: "render_2d(pos)".to_string(),
            domain: ProbeDomain::Point { at: [0.5, 0.5] },
            vary: None,
            reduce: ProbeReduce::None,
        };
        let w = emit_wrapper(&pos_probe);
        assert_eq!(
            w.text,
            "vec4 __probe_p(vec2 pos) { return (render_2d(pos)); }"
        );
        assert_eq!(&w.text[w.expr_offset..w.expr_offset + 9], "render_2d");

        let sweep_probe = ProbeSpec {
            domain: ProbeDomain::Sweep {
                var: "t".to_string(),
                from: 0.0,
                to: 1.0,
                n: 3,
            },
            ty: ProbeType::Float,
            expr: "t * 2.0".to_string(),
            ..pos_probe
        };
        let w = emit_wrapper(&sweep_probe);
        assert_eq!(w.text, "float __probe_p(float t) { return (t * 2.0); }");
    }

    #[test]
    fn remap_attributes_wrapper_lines_to_the_probe() {
        let diags = alloc::vec!["error at glsl:5:40 something".to_string()];
        let out = remap_probe_diagnostics(diags, "p", 5, 30);
        assert_eq!(out[0], "error at probe 'p' expr:1:10 something");
        // Lines before the wrapper stay untouched.
        let diags = alloc::vec!["error at glsl:2:7 something".to_string()];
        let out = remap_probe_diagnostics(diags, "p", 5, 30);
        assert_eq!(out[0], "error at glsl:2:7 something");
    }

    #[test]
    fn remap_rewrites_lowering_function_attribution() {
        let diags = alloc::vec!["in function '__probe_p': unsupported thing".to_string()];
        let out = remap_probe_diagnostics(diags, "p", 5, 30);
        assert_eq!(out[0], "in probe 'p' expr: unsupported thing");
    }
}
