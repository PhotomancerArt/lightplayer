//! Target axis enums, Target, TargetFilter, and disposition logic.

pub mod display;

pub use display::parse_target_filters;

/// Compilation/execution backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// Host LPIR JIT (`lpir-cranelift`).
    Jit,
    /// LPIR → RV32 object + linked builtins + emulator.
    Rv32,
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
/// Order: wasm, jit, rv32 — used for error messages and CLI.
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
];

/// Default targets for local `cargo test` / app runs: RV32 + WASM (Q32).
/// CI should run the full [`ALL_TARGETS`] list (see plan README / phase 05).
pub const DEFAULT_TARGETS: &[Target] = &[ALL_TARGETS[2], ALL_TARGETS[0]];

/// Partial target specification for filtering (None = wildcard).
#[derive(Debug, Clone, Default)]
pub struct TargetFilter {
    /// Filter by backend.
    pub backend: Option<Backend>,
    /// Filter by float mode.
    pub float_mode: Option<FloatMode>,
    /// Filter by ISA.
    pub isa: Option<Isa>,
    /// Filter by exec mode.
    pub exec_mode: Option<ExecMode>,
}

impl TargetFilter {
    /// Returns true if the target matches all specified filter fields.
    pub fn matches(&self, target: &Target) -> bool {
        self.backend.map_or(true, |b| b == target.backend)
            && self.float_mode.map_or(true, |f| f == target.float_mode)
            && self.isa.map_or(true, |i| i == target.isa)
            && self.exec_mode.map_or(true, |e| e == target.exec_mode)
    }
}

/// Annotation kind for test directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    /// Feature not implemented yet (temporary; expected to pass when implemented).
    Unimplemented,
    /// Known broken (bug or wrong expectation until fixed).
    Broken,
    /// Not applicable on this target — by design, not a bug (e.g. NaN on Q32, backend gap).
    Unsupported,
}

/// Annotation with optional target filter.
#[derive(Debug, Clone)]
pub struct Annotation {
    /// Kind of annotation.
    pub kind: AnnotationKind,
    /// Target filter (empty = all targets).
    pub filter: TargetFilter,
    /// Optional reason string.
    pub reason: Option<String>,
    /// Source line number.
    pub line_number: usize,
}

/// How to handle a test for a given target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// Run and expect success.
    ExpectSuccess,
    /// Run and expect failure (carries kind for stats: unimplemented vs broken).
    ExpectFailure(AnnotationKind),
    /// Skip entirely.
    Skip,
}

/// Determine disposition from file-level and directive-level annotations.
pub fn directive_disposition(
    file_annotations: &[Annotation],
    directive_annotations: &[Annotation],
    target: &Target,
) -> Disposition {
    for ann in directive_annotations.iter().chain(file_annotations.iter()) {
        if ann.filter.matches(target) {
            return match ann.kind {
                AnnotationKind::Unsupported => Disposition::Skip,
                AnnotationKind::Unimplemented | AnnotationKind::Broken => {
                    Disposition::ExpectFailure(ann.kind)
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
    fn test_target_filter_empty_matches_all() {
        let filter = TargetFilter::default();
        for target in ALL_TARGETS {
            assert!(
                filter.matches(target),
                "default filter should match {:?}",
                target
            );
        }
    }

    #[test]
    fn test_target_filter_backend() {
        let filter = TargetFilter {
            backend: Some(Backend::Wasm),
            ..Default::default()
        };
        for target in ALL_TARGETS {
            assert_eq!(
                filter.matches(target),
                target.backend == Backend::Wasm,
                "filter backend=wasm should match only wasm target"
            );
        }
    }

    #[test]
    fn test_target_filter_float_mode() {
        let filter = TargetFilter {
            float_mode: Some(FloatMode::Q32),
            ..Default::default()
        };
        for target in ALL_TARGETS {
            assert!(
                filter.matches(target),
                "filter float_mode=q32 should match all q32 targets"
            );
        }
    }

    #[test]
    fn test_target_filter_full() {
        let jit_target = &ALL_TARGETS[1];
        let filter = TargetFilter {
            backend: Some(Backend::Jit),
            float_mode: Some(FloatMode::Q32),
            isa: Some(Isa::Native),
            exec_mode: Some(ExecMode::Jit),
        };
        assert!(filter.matches(jit_target));
        assert!(!filter.matches(&ALL_TARGETS[0]));
    }

    #[test]
    fn test_target_filter_no_match() {
        let filter = TargetFilter {
            backend: Some(Backend::Wasm),
            isa: Some(Isa::Riscv32),
            ..Default::default()
        };
        for target in ALL_TARGETS {
            assert!(
                !filter.matches(target),
                "contradictory filter should match nothing"
            );
        }
    }

    #[test]
    fn test_disposition_no_annotations() {
        let target = &DEFAULT_TARGETS[0];
        let d = directive_disposition(&[], &[], target);
        assert_eq!(d, Disposition::ExpectSuccess);
    }

    #[test]
    fn test_disposition_matching_unimplemented() {
        let target = &DEFAULT_TARGETS[0];
        let ann = Annotation {
            kind: AnnotationKind::Unimplemented,
            filter: TargetFilter::default(),
            reason: None,
            line_number: 1,
        };
        let d = directive_disposition(&[ann], &[], target);
        assert_eq!(d, Disposition::ExpectFailure(AnnotationKind::Unimplemented));
    }

    #[test]
    fn test_disposition_matching_unsupported() {
        let target = &DEFAULT_TARGETS[0];
        let ann = Annotation {
            kind: AnnotationKind::Unsupported,
            filter: TargetFilter::default(),
            reason: None,
            line_number: 1,
        };
        let d = directive_disposition(&[ann], &[], target);
        assert_eq!(d, Disposition::Skip);
    }

    #[test]
    fn test_disposition_non_matching() {
        let target = ALL_TARGETS
            .iter()
            .find(|t| t.backend == Backend::Jit)
            .expect("jit target");
        let ann = Annotation {
            kind: AnnotationKind::Unsupported,
            filter: TargetFilter {
                backend: Some(Backend::Wasm),
                ..Default::default()
            },
            reason: None,
            line_number: 1,
        };
        let d = directive_disposition(&[ann], &[], target);
        assert_eq!(d, Disposition::ExpectSuccess);
    }

    #[test]
    fn test_disposition_directive_overrides_file() {
        let target = &DEFAULT_TARGETS[0];
        let file_ann = Annotation {
            kind: AnnotationKind::Unimplemented,
            filter: TargetFilter::default(),
            reason: None,
            line_number: 1,
        };
        let dir_ann = Annotation {
            kind: AnnotationKind::Unsupported,
            filter: TargetFilter::default(),
            reason: None,
            line_number: 2,
        };
        let d = directive_disposition(&[file_ann], &[dir_ann], target);
        assert_eq!(
            d,
            Disposition::Skip,
            "directive-level should be checked first"
        );
    }

    #[test]
    fn test_default_targets_is_rv32_and_wasm() {
        assert_eq!(DEFAULT_TARGETS.len(), 2);
        assert_eq!(DEFAULT_TARGETS[0].name(), "rv32.q32");
        assert_eq!(DEFAULT_TARGETS[1].name(), "wasm.q32");
    }
}
