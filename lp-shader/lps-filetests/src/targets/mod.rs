//! Target axis enums, Target, and disposition logic.

pub mod display;
pub mod target_axis;
pub mod target_selector;

pub use display::parse_target_filters;
pub use target_axis::{ALL_AXES, Axis, AxisValue, axis_value_of};
pub use target_selector::{AxisPredicate, TargetSelector};

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
/// always had a `FloatMode::F32` emit path, but nothing ever executed it.
///
/// It **is** in [`DEFAULT_TARGETS`] as of 2026-08-02. It was held out for a
/// reason that had gone stale: `@lpfn`/`@glsl` imports resolving to Q32 builtin
/// ids, so any file calling one produced an invalid module. M5 obsoleted that by
/// adding `resolve_builtin_id_for_mode` plus the tests in
/// `lpvm-wasm/src/emit/imports.rs` that pin "no import ever resolves across
/// modes" (including the load-bearing pointer-parameter case); the roadmap's G3
/// sweep identified the claim as stale, and the promotion is the follow-through.
/// Measured on this base: **850/850 files, 6,345/6,345, 0 compile-fail, 2.6 s**,
/// with `@glsl` (`builtins/trig-sin.glsl` 10/10) and `@lpfn` (`lpfn/` 89/89)
/// specifically confirmed. Unlike the `xtn.*` targets it needs no cross-target
/// artifact — wasmtime is already a test dependency.
///
/// That same stale claim also lived in `lpvm-wasm`'s runtime guards and in
/// `docs/adr/2026-08-01-float-mode-reaches-the-device.md`; both now carry the
/// measured correction. If you are about to write "wasm has no f32 builtin
/// lowering" anywhere, it is false — check `wasm.f32` first.
///
/// `xtn.f32` / `xtlpn.f32` are the **hardware-FPU** targets (roadmap M8): the
/// same `lpvm-native` backend on `IsaTarget::Xtensa` in `FloatMode::F32`,
/// emitting real `add.s`/`mul.s` on the LX7's float register file and executing
/// them in `lp-xt-emu`. They need no new match arm anywhere — the compile path
/// is already parameterised by `(isa, float_mode)` and the F32 execution path
/// goes through the typed `LpvmInstance::call` that `wasm.f32` and the rv32 f32
/// targets already use.
///
/// Like their Q32 siblings they are **not** in [`DEFAULT_TARGETS`], and for the
/// same reason: they need the Xtensa builtins image, a cross-target artifact
/// that requires the esp toolchain and is absent on a fresh clone. Defaulting
/// the f32 pair while `xtn.q32` stays on demand would be an inconsistency, not
/// a decision.
///
/// `rv32n.f32` / `rv32lpn.f32` are the **soft-float** rv32 targets (roadmap M9):
/// the same `lpvm-native` backend, compiled in `FloatMode::F32`, where every
/// float op is a call to `__addsf3` and friends inside `lp-riscv-emu`. Also
/// deliberately **not** in [`DEFAULT_TARGETS`] (roadmap Q6: rv32 f32 variants run
/// on demand). They are slow — each arithmetic op is a function call through the
/// emulator — and they are not the shipping numeric mode for any rv32 board, so
/// their cost belongs to a deliberate run, not to every `cargo test`. Select them
/// explicitly: `-t rv32lpn.f32`.
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
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Rv32fa,
        float_mode: FloatMode::F32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Lp,
        backend: Backend::Rv32fa,
        float_mode: FloatMode::F32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Naga,
        backend: Backend::Xtfa,
        float_mode: FloatMode::F32,
        isa: Isa::Xtensa,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        frontend: Frontend::Lp,
        backend: Backend::Xtfa,
        float_mode: FloatMode::F32,
        isa: Isa::Xtensa,
        exec_mode: ExecMode::Emulator,
    },
];

/// Default targets for local `cargo test` / app runs: rv32n, rv32lpn (lps-glsl
/// frontend — the primary on-device pipeline), rv32c (Cranelift), wasm (Q32),
/// interp.f32 (the CI-runnable f32 gate — host LPIR interpretation; the
/// whole corpus carries triaged f32 expectations via `run[q32]:`/`run[f32]:`
/// channels and per-target annotations), plus **`wasm.f32`**.
///
/// **`wasm.f32` joined the defaults 2026-08-02** (roadmap Q6's original intent,
/// unblocked since M5). It is the only *native-code* f32 target that can run
/// here: it needs no cross-target artifact, because wasmtime is already a test
/// dependency. Measured on this base: **850/850 files, 6,345/6,345, 2.6 s** —
/// cheap enough that keeping it on request was costing more in stale
/// explanations than in CI seconds. The `xtn.*` and `rv32*.f32` targets stay on
/// request for reasons that are still true (see [`ALL_TARGETS`]).
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
    ALL_TARGETS[8],
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
    /// The *test* does not apply here (e.g. a Q32-semantics file on an f32
    /// target). Skips like [`AnnotationKind::Unsupported`], but says the
    /// exclusion is a property of the test rather than of the backend.
    Ignore,
}

impl AnnotationKind {
    /// The keyword written after `@`.
    pub fn keyword(self) -> &'static str {
        match self {
            AnnotationKind::Unimplemented => "unimplemented",
            AnnotationKind::Unsupported => "unsupported",
            AnnotationKind::Broken => "broken",
            AnnotationKind::Ignore => "ignore",
        }
    }
}

/// One `// @kind(selector)` line.
///
/// The selector is [`TargetSelector::Name`] for the exact-target form that most
/// of the corpus uses, or an axis predicate / `*` for the scoped forms. Both
/// live in the same list and are resolved together by [`directive_disposition`].
#[derive(Debug, Clone)]
pub struct Annotation {
    /// Kind of annotation.
    pub kind: AnnotationKind,
    /// Which targets this annotation applies to.
    pub selector: TargetSelector,
    /// Source line number.
    pub line_number: usize,
}

impl Annotation {
    /// True if this annotation applies to `t`.
    pub fn applies_to(&self, t: &Target) -> bool {
        self.selector.matches(t)
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

impl Disposition {
    /// The disposition an annotation of this kind imposes.
    fn of_kind(kind: AnnotationKind) -> Disposition {
        match kind {
            AnnotationKind::Unsupported | AnnotationKind::Ignore => Disposition::Skip,
            AnnotationKind::Unimplemented => {
                Disposition::ExpectFailure(AnnotationKind::Unimplemented)
            }
            AnnotationKind::Broken => Disposition::ExpectFailure(AnnotationKind::Broken),
        }
    }
}

/// Resolve a set of annotations against one target.
///
/// **Most specific wins.** An exact target name beats any predicate; among
/// predicates, more axis terms beats fewer; `*` loses to everything. Equal
/// specificity is broken by **source order — the first annotation wins**, which
/// is exactly the old first-match-wins rule and is why a corpus of exact-name
/// annotations resolves identically to before.
///
/// Annotations of the same kind therefore union naturally: any one of them
/// matching is enough, and which one matched cannot change the outcome.
pub fn directive_disposition(directive_annotations: &[Annotation], target: &Target) -> Disposition {
    let mut best: Option<&Annotation> = None;
    for ann in directive_annotations {
        if !ann.applies_to(target) {
            continue;
        }
        let wins = match best {
            None => true,
            Some(cur) => ann.selector.specificity() > cur.selector.specificity(),
        };
        if wins {
            best = Some(ann);
        }
    }
    best.map(|a| Disposition::of_kind(a.kind))
        .unwrap_or(Disposition::ExpectSuccess)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(kind: AnnotationKind, name: &str, line: usize) -> Annotation {
        Annotation {
            kind,
            selector: TargetSelector::Name(name.to_string()),
            line_number: line,
        }
    }

    fn predicate(kind: AnnotationKind, terms: &[(&str, bool, &str)], line: usize) -> Annotation {
        let terms = terms
            .iter()
            .map(|(key, negated, value)| {
                let axis = Axis::from_key(key).expect("axis key");
                AxisPredicate {
                    value: axis.value_from_str(value).expect("axis value"),
                    negated: *negated,
                }
            })
            .collect();
        Annotation {
            kind,
            selector: TargetSelector::Predicate(terms),
            line_number: line,
        }
    }

    fn star(kind: AnnotationKind, line: usize) -> Annotation {
        Annotation {
            kind,
            selector: TargetSelector::All,
            line_number: line,
        }
    }

    #[test]
    fn test_disposition_no_annotations() {
        let target = &DEFAULT_TARGETS[0];
        let d = directive_disposition(&[], target);
        assert_eq!(d, Disposition::ExpectSuccess);
    }

    #[test]
    fn test_disposition_matching_unimplemented() {
        let target = &DEFAULT_TARGETS[0];
        let ann = named(AnnotationKind::Unimplemented, &target.name(), 1);
        let d = directive_disposition(&[ann], target);
        assert_eq!(d, Disposition::ExpectFailure(AnnotationKind::Unimplemented));
    }

    #[test]
    fn test_disposition_matching_unsupported() {
        let target = &DEFAULT_TARGETS[0];
        let ann = named(AnnotationKind::Unsupported, &target.name(), 1);
        let d = directive_disposition(&[ann], target);
        assert_eq!(d, Disposition::Skip);
    }

    #[test]
    fn test_disposition_non_matching_target() {
        let wasm = Target::from_name("wasm.q32").expect("wasm");
        let rv32c = Target::from_name("rv32c.q32").expect("rv32c");
        let ann = named(AnnotationKind::Unsupported, &wasm.name(), 1);
        let d = directive_disposition(&[ann], rv32c);
        assert_eq!(d, Disposition::ExpectSuccess);
    }

    #[test]
    fn test_ignore_skips_like_unsupported() {
        let target = Target::from_name("interp.f32").expect("interp");
        let ann = predicate(AnnotationKind::Ignore, &[("float_mode", false, "f32")], 1);
        assert_eq!(directive_disposition(&[ann], target), Disposition::Skip);
    }

    #[test]
    fn test_star_applies_to_every_target() {
        let ann = star(AnnotationKind::Unsupported, 1);
        for target in ALL_TARGETS {
            assert_eq!(
                directive_disposition(std::slice::from_ref(&ann), target),
                Disposition::Skip,
                "{}",
                target.name()
            );
        }
    }

    /// The case M1's G1 taxonomy needs and first-match-wins could not express:
    /// a whole family is unimplemented, one member of it is differently wrong.
    #[test]
    fn test_exact_name_beats_predicate_regardless_of_order() {
        let family = predicate(
            AnnotationKind::Unimplemented,
            &[("float_mode", false, "f32")],
            1,
        );
        let specific = named(AnnotationKind::Broken, "wgpu.f32", 2);
        let interp = Target::from_name("interp.f32").expect("interp");
        let wgpu = Target::from_name("wgpu.f32").expect("wgpu");

        for order in [
            vec![family.clone(), specific.clone()],
            vec![specific.clone(), family.clone()],
        ] {
            assert_eq!(
                directive_disposition(&order, interp),
                Disposition::ExpectFailure(AnnotationKind::Unimplemented),
                "the family annotation still covers the rest of f32"
            );
            assert_eq!(
                directive_disposition(&order, wgpu),
                Disposition::ExpectFailure(AnnotationKind::Broken),
                "the exact name wins on its own target"
            );
        }
    }

    /// The shape the corpus collapse relies on: a broad `@broken(frontend!=lp)`
    /// with an exact `@unsupported(wgpu.f32)` carving out the GPU tier.
    #[test]
    fn test_exact_name_carves_an_exception_out_of_a_negated_predicate() {
        let anns = vec![
            predicate(AnnotationKind::Broken, &[("frontend", true, "lp")], 1),
            named(AnnotationKind::Unsupported, "wgpu.f32", 2),
        ];
        assert_eq!(
            directive_disposition(&anns, Target::from_name("wasm.q32").unwrap()),
            Disposition::ExpectFailure(AnnotationKind::Broken)
        );
        assert_eq!(
            directive_disposition(&anns, Target::from_name("wgpu.f32").unwrap()),
            Disposition::Skip
        );
        assert_eq!(
            directive_disposition(&anns, Target::from_name("rv32lpn.q32").unwrap()),
            Disposition::ExpectSuccess
        );
    }

    #[test]
    fn test_more_terms_beats_fewer_terms() {
        let anns = vec![
            predicate(
                AnnotationKind::Unimplemented,
                &[("float_mode", false, "q32")],
                1,
            ),
            predicate(
                AnnotationKind::Broken,
                &[("float_mode", false, "q32"), ("isa", false, "xtensa")],
                2,
            ),
        ];
        assert_eq!(
            directive_disposition(&anns, Target::from_name("xtn.q32").unwrap()),
            Disposition::ExpectFailure(AnnotationKind::Broken),
            "two terms outrank one"
        );
        assert_eq!(
            directive_disposition(&anns, Target::from_name("wasm.q32").unwrap()),
            Disposition::ExpectFailure(AnnotationKind::Unimplemented)
        );
    }

    #[test]
    fn test_predicate_beats_star() {
        let anns = vec![
            star(AnnotationKind::Unsupported, 1),
            predicate(AnnotationKind::Broken, &[("backend", false, "wasm")], 2),
        ];
        assert_eq!(
            directive_disposition(&anns, Target::from_name("wasm.q32").unwrap()),
            Disposition::ExpectFailure(AnnotationKind::Broken)
        );
        assert_eq!(
            directive_disposition(&anns, Target::from_name("xtn.q32").unwrap()),
            Disposition::Skip
        );
    }

    /// Equal specificity is the old rule: the first matching annotation decides.
    /// This is what keeps a corpus of exact-name annotations byte-identical.
    #[test]
    fn test_equal_specificity_first_annotation_wins() {
        let target = Target::from_name("wasm.q32").expect("wasm");
        let first_broken = vec![
            named(AnnotationKind::Broken, "wasm.q32", 1),
            named(AnnotationKind::Unimplemented, "wasm.q32", 2),
        ];
        assert_eq!(
            directive_disposition(&first_broken, target),
            Disposition::ExpectFailure(AnnotationKind::Broken)
        );

        let first_unimpl = vec![
            named(AnnotationKind::Unimplemented, "wasm.q32", 1),
            named(AnnotationKind::Broken, "wasm.q32", 2),
        ];
        assert_eq!(
            directive_disposition(&first_unimpl, target),
            Disposition::ExpectFailure(AnnotationKind::Unimplemented)
        );

        let both_predicates = vec![
            predicate(
                AnnotationKind::Unsupported,
                &[("backend", false, "wasm")],
                1,
            ),
            predicate(AnnotationKind::Broken, &[("float_mode", false, "q32")], 2),
        ];
        assert_eq!(
            directive_disposition(&both_predicates, target),
            Disposition::Skip
        );
    }

    /// Same-kind annotations union: however many match, the answer is the kind.
    #[test]
    fn test_same_kind_annotations_union() {
        let anns = vec![
            predicate(
                AnnotationKind::Unimplemented,
                &[("float_mode", false, "q32")],
                1,
            ),
            named(AnnotationKind::Unimplemented, "wasm.q32", 2),
            star(AnnotationKind::Unimplemented, 3),
        ];
        for target in ALL_TARGETS {
            assert_eq!(
                directive_disposition(&anns, target),
                Disposition::ExpectFailure(AnnotationKind::Unimplemented),
                "{}",
                target.name()
            );
        }
    }

    #[test]
    fn test_default_targets_order_matches_const() {
        assert_eq!(DEFAULT_TARGETS.len(), 6);
        assert_eq!(DEFAULT_TARGETS[0].name(), "rv32n.q32");
        assert_eq!(DEFAULT_TARGETS[1].name(), "rv32lpn.q32");
        assert_eq!(DEFAULT_TARGETS[2].name(), "rv32c.q32");
        assert_eq!(DEFAULT_TARGETS[3].name(), "wasm.q32");
        assert_eq!(DEFAULT_TARGETS[4].name(), "interp.f32");
        assert_eq!(DEFAULT_TARGETS[5].name(), "wasm.f32");
    }
}
