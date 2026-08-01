//! Target name formatting and CLI parsing.

use super::{ALL_TARGETS, Backend, ExecMode, FloatMode, Frontend, Isa, Target};
use std::collections::BTreeSet;
use std::fmt;

/// The axis-value spellings below are the **annotation vocabulary** as well as
/// the display form: `@unsupported(backend=wasm)` matches [`Backend::Wasm`]
/// because `Backend::Wasm` writes itself as `wasm`. Each `ALL` list is what
/// `Axis::values` enumerates, and `target_axis`'s
/// `every_registered_target_is_fully_namable` test fails if a variant reaches a
/// registered target without appearing in its list.
impl Backend {
    /// Every backend, in [`super::ALL_TARGETS`] order.
    pub const ALL: &'static [Backend] = &[
        Backend::Rv32,
        Backend::Rv32fa,
        Backend::Xtfa,
        Backend::Wasm,
        Backend::Interp,
        Backend::Wgpu,
    ];
}

impl Frontend {
    /// Every frontend.
    pub const ALL: &'static [Frontend] = &[Frontend::Naga, Frontend::Lp];
}

impl FloatMode {
    /// Every float mode.
    pub const ALL: &'static [FloatMode] = &[FloatMode::Q32, FloatMode::F32];
}

impl Isa {
    /// Every ISA.
    pub const ALL: &'static [Isa] = &[Isa::Riscv32, Isa::Xtensa, Isa::Wasm32, Isa::Host];
}

impl ExecMode {
    /// Every execution mode.
    pub const ALL: &'static [ExecMode] =
        &[ExecMode::Emulator, ExecMode::Interpreter, ExecMode::Gpu];
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Backend::Rv32 => write!(f, "rv32c"),
            Backend::Rv32fa => write!(f, "rv32n"),
            Backend::Xtfa => write!(f, "xtn"),
            Backend::Wasm => write!(f, "wasm"),
            Backend::Interp => write!(f, "interp"),
            Backend::Wgpu => write!(f, "wgpu"),
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

impl fmt::Display for Frontend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Frontend::Naga => write!(f, "naga"),
            Frontend::Lp => write!(f, "lp"),
        }
    }
}

impl fmt::Display for Isa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Isa::Riscv32 => write!(f, "riscv32"),
            Isa::Xtensa => write!(f, "xtensa"),
            Isa::Wasm32 => write!(f, "wasm32"),
            Isa::Host => write!(f, "host"),
        }
    }
}

impl fmt::Display for ExecMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecMode::Emulator => write!(f, "emulator"),
            ExecMode::Interpreter => write!(f, "interpreter"),
            ExecMode::Gpu => write!(f, "gpu"),
        }
    }
}

impl Target {
    /// Canonical name (e.g. "wasm.q32").
    pub fn name(&self) -> String {
        format!("{}.{}", self.backend_name(), self.float_mode)
    }

    fn backend_name(&self) -> &'static str {
        match (self.frontend, self.backend) {
            (Frontend::Lp, Backend::Rv32fa) => "rv32lpn",
            (Frontend::Lp, Backend::Xtfa) => "xtlpn",
            (_, Backend::Rv32) => "rv32c",
            (_, Backend::Rv32fa) => "rv32n",
            (_, Backend::Xtfa) => "xtn",
            (_, Backend::Wasm) => "wasm",
            (_, Backend::Interp) => "interp",
            (_, Backend::Wgpu) => "wgpu",
        }
    }

    /// Look up target by name from [`super::ALL_TARGETS`].
    pub fn from_name(s: &str) -> Result<&'static Target, String> {
        ALL_TARGETS.iter().find(|t| t.name() == s).ok_or_else(|| {
            let valid: Vec<String> = ALL_TARGETS.iter().map(|t| t.name()).collect();
            format!("unknown target '{s}'. Valid targets: {}", valid.join(", "))
        })
    }
}

/// True if `token` selects this target: full canonical name (e.g. `wasm.q32`) or backend shorthand
/// when `token` has no `.` (e.g. `wasm` matches all wasm float modes).
fn target_matches_spec_token(token: &str, t: &Target) -> bool {
    let name = t.name();
    if name == token {
        return true;
    }
    if !token.contains('.') && t.backend_name() == token {
        return true;
    }
    false
}

/// Parse comma-separated target specs into concrete targets from [`ALL_TARGETS`], in list order.
///
/// Each token is trimmed. Empty tokens are ignored. A token matches either a full canonical name
/// or a backend shorthand when it contains no `.` (e.g. `rv32c` → `rv32c.q32`, `rv32c.f32`).
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
            let backends = "wasm, rv32c, rv32n, rv32lpn, xtn, xtlpn, interp, wgpu (shorthand) or full \
                 names like wasm.q32";
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
    fn test_target_name_wasm_q32() {
        let target = &ALL_TARGETS[0];
        assert_eq!(target.name(), "wasm.q32");
    }

    #[test]
    fn test_target_name_rv32_q32() {
        let target = &ALL_TARGETS[1];
        assert_eq!(target.name(), "rv32c.q32");
    }

    #[test]
    fn test_target_name_rv32n_q32() {
        let target = &ALL_TARGETS[2];
        assert_eq!(target.name(), "rv32n.q32");
    }

    #[test]
    fn test_target_name_rv32lpn_q32() {
        let target = &ALL_TARGETS[3];
        assert_eq!(target.name(), "rv32lpn.q32");
    }

    #[test]
    fn test_target_name_xtn_q32() {
        let t = Target::from_name("xtn.q32").expect("xtn.q32 registered");
        assert_eq!(t.name(), "xtn.q32");
        assert_eq!(t.frontend, Frontend::Naga);
        assert_eq!(t.isa, super::super::Isa::Xtensa);
    }

    /// `xtlpn` is the Xtensa **device pipeline** (lps-glsl frontend), the mirror
    /// of `rv32lpn` — the target whose greens say something about what runs on an
    /// ESP32-S3. Its name must not collide with `xtn`'s.
    #[test]
    fn test_target_name_xtlpn_q32() {
        let t = Target::from_name("xtlpn.q32").expect("xtlpn.q32 registered");
        assert_eq!(t.name(), "xtlpn.q32");
        assert_eq!(t.frontend, Frontend::Lp);
        assert_ne!(t.name(), Target::from_name("xtn.q32").unwrap().name());
    }

    #[test]
    fn test_target_from_name_valid() {
        let t = Target::from_name("wasm.q32").unwrap();
        assert_eq!(t.name(), "wasm.q32");
        let t = Target::from_name("rv32c.q32").unwrap();
        assert_eq!(t.name(), "rv32c.q32");
        let t = Target::from_name("rv32n.q32").unwrap();
        assert_eq!(t.name(), "rv32n.q32");
        let t = Target::from_name("rv32lpn.q32").unwrap();
        assert_eq!(t.name(), "rv32lpn.q32");
        let t = Target::from_name("xtn.q32").unwrap();
        assert_eq!(t.name(), "xtn.q32");
        let t = Target::from_name("xtlpn.q32").unwrap();
        assert_eq!(t.name(), "xtlpn.q32");
    }

    /// Every name in `ALL_TARGETS` round-trips and is unique. A new backend whose
    /// `backend_name` arm is missing or duplicated fails here rather than by
    /// silently answering under another target's name.
    #[test]
    fn test_all_target_names_are_unique_and_round_trip() {
        let mut seen = BTreeSet::new();
        for t in ALL_TARGETS {
            let n = t.name();
            assert!(seen.insert(n.clone()), "duplicate target name {n}");
            assert_eq!(
                Target::from_name(&n).expect("round-trips").name(),
                n,
                "{n} does not round-trip through from_name"
            );
        }
        assert_eq!(seen.len(), ALL_TARGETS.len());
    }

    /// A bare `xtn` / `xtlpn` selects **both** float modes, in `ALL_TARGETS`
    /// order — the same rule `wasm` follows, and the reason M8 registering
    /// `xtn.f32` changed what an existing shorthand means. A caller that wants
    /// only the Q32 target must spell it out; `just` recipes and CI do.
    #[test]
    fn test_parse_target_filters_xt_shorthands() {
        let v = parse_target_filters("xtn").expect("parse");
        assert_eq!(
            v.iter().map(|t| t.name()).collect::<Vec<_>>(),
            ["xtn.q32", "xtn.f32"]
        );

        let v = parse_target_filters("xtlpn").expect("parse");
        assert_eq!(
            v.iter().map(|t| t.name()).collect::<Vec<_>>(),
            ["xtlpn.q32", "xtlpn.f32"]
        );

        let v = parse_target_filters("xtn.q32,xtlpn.q32").expect("parse");
        assert_eq!(v.len(), 2);
    }

    /// The f32 pair round-trips by name, and does not collide with the Q32
    /// pair — `xtlpn` is the device pipeline (lps-glsl frontend), `xtn` is
    /// Naga, and the float mode is a separate axis from both.
    #[test]
    fn test_target_name_xt_f32_pair() {
        let t = Target::from_name("xtn.f32").expect("xtn.f32 registered");
        assert_eq!(t.name(), "xtn.f32");
        assert_eq!(t.float_mode, FloatMode::F32);
        assert_eq!(t.isa, Isa::Xtensa);

        let t = Target::from_name("xtlpn.f32").expect("xtlpn.f32 registered");
        assert_eq!(t.name(), "xtlpn.f32");
        assert_eq!(t.frontend, Frontend::Lp);
        assert_ne!(t.name(), Target::from_name("xtn.f32").unwrap().name());
    }

    #[test]
    fn test_target_from_name_invalid() {
        let err = Target::from_name("invalid").unwrap_err();
        assert!(err.contains("unknown target"));
        assert!(err.contains("wasm.q32"));
        assert!(err.contains("rv32c.q32"));
        assert!(err.contains("rv32n.q32"));
        assert!(err.contains("rv32lpn.q32"));
        assert!(err.contains("xtn.q32"));
        assert!(err.contains("xtlpn.q32"));
    }

    /// A bare backend token selects **every** float mode that backend has, in
    /// `ALL_TARGETS` order — so `wasm` is three targets once `wasm.f32` exists,
    /// not two. Callers that mean one mode must spell it out.
    #[test]
    fn test_parse_target_filters_comma_and_shorthand() {
        let v = parse_target_filters("rv32n,wasm").expect("parse");
        let names: Vec<String> = v.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            vec!["rv32n.q32", "rv32n.f32", "wasm.q32", "wasm.f32"]
        );
    }

    #[test]
    fn test_parse_target_filters_backend_single() {
        let v = parse_target_filters("rv32c").expect("parse");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name(), "rv32c.q32");
    }

    #[test]
    fn test_target_name_wasm_f32() {
        let t = Target::from_name("wasm.f32").expect("wasm.f32 registered");
        assert_eq!(t.name(), "wasm.f32");
        assert_eq!(t.float_mode, FloatMode::F32);
        assert_eq!(t.backend, Backend::Wasm);
        assert_eq!(t.isa, super::super::Isa::Wasm32);
    }

    /// The `wasm` shorthand now selects **both** float modes. A test run that
    /// meant only the Q32 wasm target must say `wasm.q32`.
    #[test]
    fn test_parse_target_filters_wasm_shorthand_covers_both_modes() {
        let v = parse_target_filters("wasm").expect("parse");
        let names: Vec<String> = v.iter().map(|t| t.name()).collect();
        assert_eq!(names, vec!["wasm.q32", "wasm.f32"]);
    }

    /// Like `wasm`, the rv32 backend shorthands select every registered float
    /// mode. A caller that meant the shipping fixed-point target must spell
    /// `rv32n.q32` — the soft-float sibling is orders of magnitude slower.
    #[test]
    fn test_parse_target_filters_rv32n_shorthand() {
        let v = parse_target_filters("rv32n").expect("parse");
        let names: Vec<String> = v.iter().map(|t| t.name()).collect();
        assert_eq!(names, vec!["rv32n.q32", "rv32n.f32"]);
    }

    #[test]
    fn test_parse_target_filters_rv32lpn_shorthand() {
        let v = parse_target_filters("rv32lpn").expect("parse");
        let names: Vec<String> = v.iter().map(|t| t.name()).collect();
        assert_eq!(names, vec!["rv32lpn.q32", "rv32lpn.f32"]);
    }

    /// The soft-float rv32 targets exist and name the same backend as their
    /// Q32 siblings — only `float_mode` differs, which is what makes an
    /// `@unimplemented(float_mode=f32)` predicate cover them.
    #[test]
    fn test_target_name_rv32_f32_pair() {
        for (name, frontend) in [("rv32n.f32", Frontend::Naga), ("rv32lpn.f32", Frontend::Lp)] {
            let t = Target::from_name(name).expect("registered");
            assert_eq!(t.name(), name);
            assert_eq!(t.float_mode, FloatMode::F32);
            assert_eq!(t.backend, Backend::Rv32fa);
            assert_eq!(t.frontend, frontend);
            assert_eq!(t.isa, super::super::Isa::Riscv32);
        }
    }

    #[test]
    fn test_parse_target_filters_full_name() {
        let v = parse_target_filters("wasm.q32").expect("parse");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name(), "wasm.q32");
    }

    #[test]
    fn test_parse_target_filters_rejects_unknown_token() {
        let e = parse_target_filters("not-a-backend").unwrap_err();
        assert!(e.contains("not-a-backend"));
    }

    #[test]
    fn test_parse_target_filters_rejects_legacy_rv32lp() {
        let e = parse_target_filters("rv32lp").unwrap_err();
        assert!(e.contains("rv32lp"));
    }
}
