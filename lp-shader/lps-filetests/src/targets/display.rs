//! Target name formatting and CLI parsing.

use super::{ALL_TARGETS, Backend, FloatMode, Target};
use std::collections::BTreeSet;
use std::fmt;

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Backend::Jit => write!(f, "jit"),
            Backend::Rv32 => write!(f, "rv32"),
            Backend::Rv32lp => write!(f, "rv32lp"),
            Backend::Wasm => write!(f, "wasm"),
        }
    }
}

impl fmt::Display for FloatMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FloatMode::Q32 => write!(f, "q32"),
            FloatMode::F32 => write!(f, "f32"),
        }
    }
}

impl Target {
    /// Canonical name (e.g. "jit.q32").
    pub fn name(&self) -> String {
        format!("{}.{}", self.backend, self.float_mode)
    }

    /// Look up target by name from [`super::ALL_TARGETS`].
    pub fn from_name(s: &str) -> Result<&'static Target, String> {
        ALL_TARGETS.iter().find(|t| t.name() == s).ok_or_else(|| {
            let valid: Vec<String> = ALL_TARGETS.iter().map(|t| t.name()).collect();
            format!("unknown target '{s}'. Valid targets: {}", valid.join(", "))
        })
    }
}

/// True if `token` selects this target: full canonical name (e.g. `jit.q32`) or backend shorthand
/// when `token` has no `.` (e.g. `jit` matches all JIT float modes).
fn target_matches_spec_token(token: &str, t: &Target) -> bool {
    let name = t.name();
    if name == token {
        return true;
    }
    if !token.contains('.') && format!("{}", t.backend) == token {
        return true;
    }
    false
}

/// Parse comma-separated target specs into concrete targets from [`ALL_TARGETS`], in list order.
///
/// Each token is trimmed. Empty tokens are ignored. A token matches either a full canonical name
/// or a backend shorthand when it contains no `.` (e.g. `rv32` → `rv32.q32`, `rv32.f32`).
pub fn parse_target_filters(spec: &str) -> Result<Vec<&'static Target>, String> {
    let mut chosen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<&'static Target> = Vec::new();

    for raw in spec.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let mut any = false;
        for t in ALL_TARGETS {
            if target_matches_spec_token(token, t) {
                any = true;
                let n = t.name();
                if chosen.insert(n.clone()) {
                    out.push(t);
                }
            }
        }
        if !any {
            let valid: Vec<String> = ALL_TARGETS.iter().map(|t| t.name()).collect();
            let backends = "jit, wasm, rv32, rv32lp (shorthand) or full names like jit.q32";
            return Err(format!(
                "unknown target '{token}'. Try {backends}. Known targets: {}",
                valid.join(", ")
            ));
        }
    }

    if out.is_empty() {
        return Err("no targets selected (empty --target?)".to_string());
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_name_jit_q32() {
        let target = &ALL_TARGETS[1];
        assert_eq!(target.name(), "jit.q32");
    }

    #[test]
    fn test_target_name_wasm_q32() {
        let target = &ALL_TARGETS[0];
        assert_eq!(target.name(), "wasm.q32");
    }

    #[test]
    fn test_target_name_rv32_q32() {
        let target = &ALL_TARGETS[2];
        assert_eq!(target.name(), "rv32.q32");
    }

    #[test]
    fn test_target_name_rv32lp_q32() {
        let target = &ALL_TARGETS[3];
        assert_eq!(target.name(), "rv32lp.q32");
    }

    #[test]
    fn test_target_from_name_valid() {
        let t = Target::from_name("jit.q32").unwrap();
        assert_eq!(t.name(), "jit.q32");
        let t = Target::from_name("wasm.q32").unwrap();
        assert_eq!(t.name(), "wasm.q32");
        let t = Target::from_name("rv32.q32").unwrap();
        assert_eq!(t.name(), "rv32.q32");
        let t = Target::from_name("rv32lp.q32").unwrap();
        assert_eq!(t.name(), "rv32lp.q32");
    }

    #[test]
    fn test_target_from_name_invalid() {
        let err = Target::from_name("invalid").unwrap_err();
        assert!(err.contains("unknown target"));
        assert!(err.contains("jit.q32"));
        assert!(err.contains("wasm.q32"));
        assert!(err.contains("rv32.q32"));
        assert!(err.contains("rv32lp.q32"));
    }

    #[test]
    fn test_parse_target_filters_comma_and_shorthand() {
        let v = parse_target_filters("jit,wasm").expect("parse");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name(), "jit.q32");
        assert_eq!(v[1].name(), "wasm.q32");
    }

    #[test]
    fn test_parse_target_filters_backend_single() {
        let v = parse_target_filters("rv32").expect("parse");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name(), "rv32.q32");
    }

    #[test]
    fn test_parse_target_filters_rv32lp_shorthand() {
        let v = parse_target_filters("rv32lp").expect("parse");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name(), "rv32lp.q32");
    }

    #[test]
    fn test_parse_target_filters_full_name() {
        let v = parse_target_filters("jit.q32").expect("parse");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name(), "jit.q32");
    }

    #[test]
    fn test_parse_target_filters_rejects_unknown_token() {
        let e = parse_target_filters("not-a-backend").unwrap_err();
        assert!(e.contains("not-a-backend"));
    }
}
