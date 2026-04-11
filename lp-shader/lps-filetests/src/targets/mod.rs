//! Target axis enums, Target, and disposition logic.

pub mod display;

pub use display::parse_target_filters;

/// Compilation/execution backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// Host LPIR JIT (`lpvm-cranelift`).
    Jit,
    /// LPIR → RV32 via Cranelift + linked builtins + emulator.
    Rv32,
    /// LPIR → RV32 via native backend + linked builtins + emulator.
    Rv32lp,
    /// LPIR → RV32 via fastalloc native backend + linked builtins + emulator.
    Rv32fa,
    /// WebAssembly via wasmtime.
    Wasm,
}

/// Instruction set architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Isa {
    /// RISC-V 32-bit.
    Riscv32,
    /// WebAssembly 32-bit.
    Wasm32,
    /// Native host.
    Native,
}

/// Execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecMode {
    /// JIT on host.
    Jit,
    /// Emulator (e.g. RISC-V emulator).
    Emulator,
}

/// Floating-point mode (Q32 fixed-point or F32 native).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatMode {
    /// 32-bit fixed-point Q16.16.
    Q32,
    /// 32-bit native float.
    F32,
}

/// Concrete target configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    /// Backend to use.
    pub backend: Backend,
    /// Float representation.
    pub float_mode: FloatMode,
    /// Instruction set.
    pub isa: Isa,
    /// How to execute.
    pub exec_mode: ExecMode,
}

/// All supported targets (`Target::from_name` searches this list).
/// Order: wasm, jit, rv32, rv32lp, rv32fa — used for error messages and CLI.
pub const ALL_TARGETS: &[Target] = &[
    Target {
        backend: Backend::Wasm,
        float_mode: FloatMode::Q32,
        isa: Isa::Wasm32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        backend: Backend::Jit,
        float_mode: FloatMode::Q32,
        isa: Isa::Native,
        exec_mode: ExecMode::Jit,
    },
    Target {
        backend: Backend::Rv32,
        float_mode: FloatMode::Q32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        backend: Backend::Rv32lp,
        float_mode: FloatMode::Q32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        backend: Backend::Rv32fa,
        float_mode: FloatMode::Q32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
];

/// Default targets for local `cargo test` / app runs: rv32lp, rv32 (Cranelift), wasm (Q32).
/// CI should run the full [`ALL_TARGETS`] list (see plan README / phase 05).
pub const DEFAULT_TARGETS: &[Target] = &[ALL_TARGETS[3], ALL_TARGETS[2], ALL_TARGETS[0]];

/// Annotation kind for test directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    /// Feature not implemented yet (temporary; expected to pass when implemented).
    Unimplemented,
    /// Not applicable on this target — by design, not a bug (e.g. NaN on Q32, backend gap).
    Unsupported,
}

/// Per-directive annotation: exact canonical target name (e.g. `wasm.q32`).
#[derive(Debug, Clone)]
pub struct Annotation {
    /// Kind of annotation.
    pub kind: AnnotationKind,
    /// Canonical target name from [`Target::name`].
    pub target: String,
    /// Source line number.
    pub line_number: usize,
}

impl Annotation {
    /// True if this annotation applies to `t`.
    pub fn applies_to(&self, t: &Target) -> bool {
        self.target == t.name()
    }
}

/// How to handle a test for a given target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Run and expect success.
    ExpectSuccess,
    /// Run and expect failure (unimplemented on this target).
    ExpectFailure(AnnotationKind),
    /// Skip entirely (unsupported).
    Skip,
}

/// Determine disposition from directive-level annotations only.
pub fn directive_disposition(directive_annotations: &[Annotation], target: &Target) -> Disposition {
    for ann in directive_annotations {
        if ann.applies_to(target) {
            return match ann.kind {
                AnnotationKind::Unsupported => Disposition::Skip,
                AnnotationKind::Unimplemented => {
                    Disposition::ExpectFailure(AnnotationKind::Unimplemented)
                }
            };
        }
    }
    Disposition::ExpectSuccess
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disposition_no_annotations() {
        let target = &DEFAULT_TARGETS[0];
        let d = directive_disposition(&[], target);
        assert_eq!(d, Disposition::ExpectSuccess);
    }

    #[test]
    fn test_disposition_matching_unimplemented() {
        let target = &DEFAULT_TARGETS[0];
        let ann = Annotation {
            kind: AnnotationKind::Unimplemented,
            target: target.name(),
            line_number: 1,
        };
        let d = directive_disposition(&[ann], target);
        assert_eq!(d, Disposition::ExpectFailure(AnnotationKind::Unimplemented));
    }

    #[test]
    fn test_disposition_matching_unsupported() {
        let target = &DEFAULT_TARGETS[0];
        let ann = Annotation {
            kind: AnnotationKind::Unsupported,
            target: target.name(),
            line_number: 1,
        };
        let d = directive_disposition(&[ann], target);
        assert_eq!(d, Disposition::Skip);
    }

    #[test]
    fn test_disposition_non_matching_target() {
        let wasm = Target::from_name("wasm.q32").expect("wasm");
        let rv32 = Target::from_name("rv32.q32").expect("rv32");
        let ann = Annotation {
            kind: AnnotationKind::Unsupported,
            target: wasm.name(),
            line_number: 1,
        };
        let d = directive_disposition(&[ann], rv32);
        assert_eq!(d, Disposition::ExpectSuccess);
    }

    #[test]
    fn test_default_targets_order_matches_const() {
        assert_eq!(DEFAULT_TARGETS.len(), 3);
        assert_eq!(DEFAULT_TARGETS[0].name(), "rv32lp.q32");
        assert_eq!(DEFAULT_TARGETS[1].name(), "rv32.q32");
        assert_eq!(DEFAULT_TARGETS[2].name(), "wasm.q32");
    }
}
