//! Target axis enums, Target, and disposition logic.

pub mod display;

pub use display::parse_target_filters;

/// Compilation/execution backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// LPIR → RV32 via Cranelift + linked builtins + emulator.
    Rv32,
    /// LPIR → RV32 via `lpvm-native` + linked builtins + emulator.
    Rv32fa,
    /// LPIR → Xtensa via `lpvm-native` + linked builtins + emulator
    /// (`lp-xt-emu` on the ESP32-S3 board profile).
    ///
    /// Same engine as [`Backend::Rv32fa`] — `rt_emu` takes the ISA as a runtime
    /// parameter — so this variant only picks `IsaTarget::Xtensa` at engine
    /// construction. See `docs/adr/2026-07-30-isa-parameterized-host-emu-engine.md`.
    Xtfa,
    /// WebAssembly via wasmtime.
    Wasm,
    /// Host-side LPIR interpreter (`lpir::interpret`), f32 semantics; no codegen.
    Interp,
    /// GPU probe via lp-gfx-wgpu (naga glsl-in -> wgsl-out, fragment render).
    Wgpu,
}

/// GLSL frontend used before LPIR backend compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Frontend {
    /// Existing Naga-based frontend.
    Naga,
    /// New LightPlayer-shaped GLSL frontend.
    Lp,
}

/// Instruction set architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Isa {
    /// RISC-V 32-bit.
    Riscv32,
    /// Xtensa (ESP32-S3 / LX7 and classic ESP32 / LX6 — ISA-identical for the
    /// emitted integer subset). Filetests run the S3 board profile.
    Xtensa,
    /// WebAssembly 32-bit.
    Wasm32,
    /// Host CPU (no guest ISA; LPIR is interpreted directly).
    Host,
}

/// Execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecMode {
    /// Emulator (e.g. RISC-V emulator) or wasmtime.
    Emulator,
    /// Direct LPIR interpretation on the host (no compiled artifact).
    Interpreter,
    /// Fragment render on a wgpu device (adapter-gated).
    Gpu,
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
    /// Frontend used to lower GLSL to LPIR.
    pub frontend: Frontend,
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
/// Order: wasm, rv32c, rv32n, rv32lpn, interp, wgpu, xtn, xtlpn, wasm.f32 — used
/// for error messages and CLI.
///
/// Everything after `rv32lpn.q32` is **appended**, deliberately: [`DEFAULT_TARGETS`]
/// indexes into this list, so inserting anywhere else would silently repoint the
/// defaults. `test_default_targets_order_matches_const` is the guard.
///
/// `wasm.f32` is the first *native-code* f32 target: the `lpvm-wasm` backend has
/// always had a `FloatMode::F32` emit path, but nothing ever executed it. It is
/// deliberately **not** in [`DEFAULT_TARGETS`] — see the corpus findings from the
/// run that first exercised it (`@lpfn`/`@glsl` builtin imports still resolve to
/// the Q32 builtin ids, so any file calling one produces an invalid module).
pub const ALL_TARGETS: &[Target] = &[
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Wasm,
        float_mode: FloatMode::Q32,
        isa: Isa::Wasm32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Rv32,
        float_mode: FloatMode::Q32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Rv32fa,
        float_mode: FloatMode::Q32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Lp,
        backend: Backend::Rv32fa,
        float_mode: FloatMode::Q32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Interp,
        float_mode: FloatMode::F32,
        isa: Isa::Host,
        exec_mode: ExecMode::Interpreter,
    },
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Wgpu,
        float_mode: FloatMode::F32,
        isa: Isa::Host,
        exec_mode: ExecMode::Gpu,
    },
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Xtfa,
        float_mode: FloatMode::Q32,
        isa: Isa::Xtensa,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Lp,
        backend: Backend::Xtfa,
        float_mode: FloatMode::Q32,
        isa: Isa::Xtensa,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Wasm,
        float_mode: FloatMode::F32,
        isa: Isa::Wasm32,
        exec_mode: ExecMode::Emulator,
    },
];

/// Default targets for local `cargo test` / app runs: rv32n, rv32lpn (lps-glsl
/// frontend — the primary on-device pipeline), rv32c (Cranelift), wasm (Q32),
/// plus interp.f32 (the CI-runnable f32 gate — host LPIR interpretation; the
/// whole corpus carries triaged f32 expectations via `run[q32]:`/`run[f32]:`
/// channels and per-target annotations).
///
/// **`xtn.q32` / `xtlpn.q32` are deliberately NOT here.** They are in
/// [`ALL_TARGETS`] and run on request (`-t xtn.q32`). Two reasons: they need the
/// Xtensa builtins image, a cross-target artifact that requires the esp toolchain
/// and is absent on a fresh clone; and defaulting them is a cost decision to make
/// against a measured number, not by assumption. Select them explicitly until
/// that measurement says otherwise.
pub const DEFAULT_TARGETS: &[Target] = &[
    ALL_TARGETS[2],
    ALL_TARGETS[3],
    ALL_TARGETS[1],
    ALL_TARGETS[0],
    ALL_TARGETS[4],
];

/// Annotation kind for test directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    /// Feature not implemented yet (temporary; expected to pass when implemented).
    Unimplemented,
    /// Not applicable on this target — by design, not a bug (e.g. NaN on Q32, backend gap).
    Unsupported,
    /// Known broken — test is expected to fail due to a known bug.
    Broken,
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
                AnnotationKind::Broken => Disposition::ExpectFailure(AnnotationKind::Broken),
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
        let rv32c = Target::from_name("rv32c.q32").expect("rv32c");
        let ann = Annotation {
            kind: AnnotationKind::Unsupported,
            target: wasm.name(),
            line_number: 1,
        };
        let d = directive_disposition(&[ann], rv32c);
        assert_eq!(d, Disposition::ExpectSuccess);
    }

    #[test]
    fn test_default_targets_order_matches_const() {
        assert_eq!(DEFAULT_TARGETS.len(), 5);
        assert_eq!(DEFAULT_TARGETS[0].name(), "rv32n.q32");
        assert_eq!(DEFAULT_TARGETS[1].name(), "rv32lpn.q32");
        assert_eq!(DEFAULT_TARGETS[2].name(), "rv32c.q32");
        assert_eq!(DEFAULT_TARGETS[3].name(), "wasm.q32");
        assert_eq!(DEFAULT_TARGETS[4].name(), "interp.f32");
    }
}
