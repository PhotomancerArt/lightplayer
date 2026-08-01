//! Register class of each virtual register and each instruction operand.
//!
//! The allocator runs one pool per [`RegClass`], so every allocation decision
//! needs to know which class it is deciding for. This module is the single
//! place that answers.
//!
//! # Why the class comes off the instruction, not off the LPIR type
//!
//! The obvious source would be the vreg's LPIR type — `IrType::F32` → float.
//! It is the wrong one. In Q16.16 mode a GLSL `float` **is an integer**: it is
//! a fixed-point value that lives in a GPR and is added with `ADD`. The same
//! LPIR `F32` value in native-f32 mode lives in an FPR and is added with a
//! hardware FPU instruction. Class is therefore a function of
//! `(type, float_mode)`, never of type alone.
//!
//! Lowering already evaluates exactly that pair when it picks which [`VInst`]
//! to emit. Reading the class back off the chosen instruction reuses that
//! decision instead of re-deriving it, which is why the allocator never has to
//! know which float mode it is running under.
//!
//! Every integer instruction the backend emits answers [`RegClass::Int`] for
//! every operand — including every `f32` in a Q32 shader, which is the correct
//! answer and not a placeholder. Only the hardware-float `VInst`s introduced by
//! M7 (`FAluRRR`, `Wfr`, …) put anything in [`RegClass::Float`], and only some
//! of *their* operands: the transfers and the comparison are deliberately
//! mixed-class, because that is what the calling convention is made of.
//!
//! # `Call` is Int-only, on purpose
//!
//! There are no float arms for [`VInst::Call`] or [`VInst::Ret`], and that is a
//! decision rather than an omission (M7 D1/D2). Float values travel across
//! every call and return boundary in **address registers**, as raw IEEE bit
//! patterns, because the toolchain that compiles the float builtins we call
//! does exactly that. Lowering inserts explicit [`VInst::Rfr`]/[`VInst::Wfr`]
//! transfers at those four boundaries, so by the time a `Call` is built its
//! operands really are integers, and the ABI is legible in the VInst dump
//! rather than implied by a table here.

use alloc::vec::Vec;

use crate::abi::RegClass;
use crate::vinst::{VInst, VReg};

/// Register class of the `def_idx`-th value `inst` defines.
///
/// `def_idx` counts defs in [`VInst::for_each_def`] order. It is a parameter
/// rather than an instruction-wide answer because a single call can return
/// values of mixed class once float returns exist.
#[cfg(feature = "float-f32")]
pub fn def_class(inst: &VInst, def_idx: usize) -> RegClass {
    let _ = def_idx;
    match inst {
        // The float file is where the value lands.
        VInst::FAluRRR { .. }
        | VInst::FAluRR { .. }
        | VInst::FSelect { .. }
        | VInst::FLoad32 { .. }
        | VInst::IToF { .. }
        // `Wfr` is the AR → FR half of the boundary transfer: its *result* is
        // the float.
        | VInst::Wfr { .. } => RegClass::Float,

        // `Fcmp` yields an ordinary 0/1 integer, so its consumers (`BrIf`,
        // `Select`, integer arithmetic) need no float awareness at all. On
        // Xtensa the comparison writes a Boolean register and the emitter
        // materializes the 0/1 into an AR inside the same sequence (M7 D5).
        VInst::Fcmp { .. }
        // `Rfr` is the FR → AR half: its result is the integer bit pattern.
        | VInst::Rfr { .. } => RegClass::Int,

        _ => RegClass::Int,
    }
}

/// Register class the `use_idx`-th operand of `inst` must be supplied in.
///
/// `use_idx` counts uses in [`VInst::for_each_use`] order — the same order
/// [`VInst::for_each_use`] visits them, which is why the mixed-class
/// instructions below index on it rather than answering instruction-wide.
#[cfg(feature = "float-f32")]
pub fn use_class(inst: &VInst, use_idx: usize) -> RegClass {
    match inst {
        // Both operands come out of the float file.
        VInst::FAluRRR { .. } | VInst::Fcmp { .. } => RegClass::Float,
        VInst::FAluRR { .. } => RegClass::Float,

        // `cond` is an integer 0/1 (typically an `Fcmp` or `Icmp` result); the
        // two candidate values are floats. Use order is (cond, if_true,
        // if_false).
        VInst::FSelect { .. } => {
            if use_idx == 0 {
                RegClass::Int
            } else {
                RegClass::Float
            }
        }

        // Addresses are always integers; only the loaded/stored value is float.
        // Use order for `FStore32` is (src, base).
        VInst::FLoad32 { .. } => RegClass::Int,
        VInst::FStore32 { .. } => {
            if use_idx == 0 {
                RegClass::Float
            } else {
                RegClass::Int
            }
        }

        // The boundary transfers, each reading the file the other one writes.
        // Claiming the wrong one here is the exact silent bit-reinterpretation
        // `verify::verify_operand_classes` was built to catch.
        VInst::Wfr { .. } => RegClass::Int,
        VInst::Rfr { .. } => RegClass::Float,

        // Integer → float conversion reads an integer.
        VInst::IToF { .. } => RegClass::Int,

        _ => RegClass::Int,
    }
}

/// Without `float-f32` there is no float lowering linked, so no float `VInst`
/// can be constructed and the answer is [`RegClass::Int`] for every operand of
/// every instruction — **as a constant, not as a match that happens to return
/// one every time**.
///
/// That distinction is the whole reason these two functions are gated rather
/// than left as one implementation. A constant lets the optimizer delete the
/// call, then [`VRegClasses`]'s table, then the per-class pool machinery that
/// consumes it. A match over `&VInst` does not: LLVM cannot prove the float
/// arms are unreachable, so the class-aware allocator becomes live code in a
/// Fixed-only image. **Measured: +496 B on the ESP32-C6 image** when these were
/// left ungated.
///
/// This is the same shape as [`crate::lower::builtin_mode`]'s gate, and it is
/// the second time on this roadmap that a *runtime-valued* query in a
/// gated-feature's shared path defeated the gate. Any new "which class / which
/// mode" query on this path needs the same treatment.
#[cfg(not(feature = "float-f32"))]
pub fn def_class(inst: &VInst, def_idx: usize) -> RegClass {
    let _ = (inst, def_idx);
    RegClass::Int
}

/// See [`def_class`]'s note — same gate, same measured reason.
#[cfg(not(feature = "float-f32"))]
pub fn use_class(inst: &VInst, use_idx: usize) -> RegClass {
    let _ = (inst, use_idx);
    RegClass::Int
}

/// The register class of every virtual register in one function.
///
/// A vreg has exactly one class for its whole life: it is defined once, and the
/// defining instruction fixes the register file the value lives in. Uses must
/// agree, which [`crate::regalloc::verify`] checks.
///
/// Built by [`build_operand_layout`](crate::regalloc::walk) rather than by a
/// pass of its own. That pass already visits every def of every instruction, and
/// `VInst::for_each_def` is generic over its callback — a second walk would mean
/// a second monomorphization of a match over the whole instruction set, which is
/// real flash in a compiler that ships on the device.
#[derive(Debug, Clone, Default)]
pub struct VRegClasses {
    /// Indexed by vreg; short of the true vreg count whenever the tail is all
    /// integer, which [`VRegClasses::of`] handles.
    classes: Vec<RegClass>,
}

impl VRegClasses {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `vreg`'s defining instruction puts it in `class`.
    ///
    /// [`RegClass::Int`] records **nothing**: it is already the answer for any
    /// vreg the table does not cover, and a vreg is defined once, so there is no
    /// earlier non-integer entry that would need overwriting.
    ///
    /// That is not just tidiness. It makes the integer path provably empty, so
    /// on a build where the class is a compile-time integer the optimizer
    /// deletes this call and the table with it — which is what keeps a
    /// class-aware allocator from charging the Q32-only firmware for a
    /// capability it does not use.
    pub fn record_def(&mut self, vreg: VReg, class: RegClass) {
        if class == RegClass::Int {
            return;
        }
        let idx = vreg.0 as usize;
        if idx >= self.classes.len() {
            self.classes.resize(idx + 1, RegClass::Int);
        }
        self.classes[idx] = class;
    }

    /// Class of `vreg`; [`RegClass::Int`] for anything outside the table.
    ///
    /// Out of range means "never recorded as anything but integer" — either a
    /// vreg with no def in the stream (an entry parameter or the vmctx pointer,
    /// integer on every ABI we target) or one past the recorded tail.
    pub fn of(&self, vreg: VReg) -> RegClass {
        self.classes
            .get(vreg.0 as usize)
            .copied()
            .unwrap_or(RegClass::Int)
    }

    /// Derive every vreg's class by walking the instruction stream.
    ///
    /// Test-only: production builds fill the table from
    /// [`build_operand_layout`](crate::regalloc::walk)'s existing walk, and this
    /// exists so class derivation can be exercised on its own.
    #[cfg(test)]
    pub fn compute(vinsts: &[VInst], vreg_pool: &[VReg], func_abi: &crate::abi::FuncAbi) -> Self {
        let _ = func_abi;
        let mut classes = Self::new();
        for inst in vinsts {
            let mut def_idx = 0usize;
            inst.for_each_def(vreg_pool, |def_vreg| {
                classes.record_def(def_vreg, def_class(inst, def_idx));
                def_idx += 1;
            });
        }
        classes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regalloc::test::abi_fixtures;
    use crate::vinst::{AluOp, SRC_OP_NONE};

    fn vinsts() -> Vec<VInst> {
        vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Add,
                dst: VReg(1),
                src1: VReg(0),
                src2: VReg(0),
                src_op: SRC_OP_NONE,
            },
        ]
    }

    /// Q32 is an integer world: every vreg, including every GLSL `float`, is
    /// integer-class. This is the invariant the f32 milestones must not break
    /// for the fixed-point path.
    #[test]
    fn every_vreg_is_integer_class_today() {
        let insts = vinsts();
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi());
        for v in 0..4u16 {
            assert_eq!(classes.of(VReg(v)), RegClass::Int);
        }
    }

    #[test]
    fn unknown_vregs_answer_int() {
        let classes = VRegClasses::compute(&[], &[], &abi_fixtures::void_func_abi());
        assert_eq!(classes.of(VReg(9999)), RegClass::Int);
    }

    /// An all-integer function must not allocate a class table at all — that is
    /// what keeps the class-aware allocator free for the Q32 path.
    #[test]
    fn an_all_integer_function_allocates_no_table() {
        let insts = vinsts();
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi());
        assert_eq!(classes.classes.capacity(), 0);
    }

    #[test]
    fn recording_a_float_def_grows_the_table_and_reads_back() {
        let mut classes = VRegClasses::new();
        classes.record_def(VReg(3), RegClass::Float);
        assert_eq!(classes.of(VReg(3)), RegClass::Float);
        // Everything the growth skipped over stays integer.
        for v in [0u16, 1, 2, 4, 500] {
            assert_eq!(classes.of(VReg(v)), RegClass::Int);
        }
    }

    /// Recording integer is a no-op, which is what makes the whole table
    /// disappear on an all-integer function.
    #[test]
    fn recording_an_integer_def_allocates_nothing() {
        let mut classes = VRegClasses::new();
        classes.record_def(VReg(9), RegClass::Int);
        assert_eq!(classes.classes.capacity(), 0);
        assert_eq!(classes.of(VReg(9)), RegClass::Int);
    }

    #[test]
    fn operand_classes_match_the_defining_instruction() {
        let insts = vinsts();
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi());
        assert_eq!(def_class(&insts[1], 0), classes.of(VReg(1)));
        assert_eq!(use_class(&insts[1], 0), classes.of(VReg(0)));
    }

    // ── The float class map ──────────────────────────────────────────────────
    //
    // These are the phase's real assertions. Nothing constructs a float `VInst`
    // yet, so a wrong entry here would be invisible until the emitter read a
    // float out of an address register — silently, as a plausible wrong number.

    use crate::vinst::{FAluOp, FAluRROp, FcmpCond};

    const V: [VReg; 4] = [VReg(0), VReg(1), VReg(2), VReg(3)];

    /// Assert the full operand-class signature of one instruction: the classes
    /// of its defs, then of its uses, in `for_each_def` / `for_each_use` order.
    fn assert_signature(inst: &VInst, defs: &[RegClass], uses: &[RegClass]) {
        let mut n_defs = 0usize;
        inst.for_each_def(&[], |_| n_defs += 1);
        let mut n_uses = 0usize;
        inst.for_each_use(&[], |_| n_uses += 1);
        assert_eq!(n_defs, defs.len(), "{inst:?}: def count");
        assert_eq!(n_uses, uses.len(), "{inst:?}: use count");
        for (i, want) in defs.iter().enumerate() {
            assert_eq!(def_class(inst, i), *want, "{inst:?}: def {i}");
        }
        for (i, want) in uses.iter().enumerate() {
            assert_eq!(use_class(inst, i), *want, "{inst:?}: use {i}");
        }
    }

    const F: RegClass = RegClass::Float;
    const I: RegClass = RegClass::Int;

    /// Arithmetic is float all the way through.
    #[test]
    fn float_arithmetic_is_float_in_every_operand() {
        assert_signature(
            &VInst::FAluRRR {
                op: FAluOp::Add,
                dst: V[0],
                src1: V[1],
                src2: V[2],
                src_op: SRC_OP_NONE,
            },
            &[F],
            &[F, F],
        );
        assert_signature(
            &VInst::FAluRR {
                op: FAluRROp::Abs,
                dst: V[0],
                src: V[1],
                src_op: SRC_OP_NONE,
            },
            &[F],
            &[F],
        );
    }

    /// A compare reads floats and writes an **integer** 0/1. That asymmetry is
    /// what lets `BrIf`, `Select` and integer arithmetic consume a float
    /// comparison with no float awareness of their own.
    #[test]
    fn compare_reads_float_and_writes_int() {
        assert_signature(
            &VInst::Fcmp {
                dst: V[0],
                lhs: V[1],
                rhs: V[2],
                cond: FcmpCond::Lt,
                src_op: SRC_OP_NONE,
            },
            &[I],
            &[F, F],
        );
    }

    /// `FSelect`'s condition is an integer and its two candidates are floats —
    /// the one instruction where a use-index off-by-one swaps register files.
    #[test]
    fn fselect_mixes_an_int_condition_with_float_values() {
        assert_signature(
            &VInst::FSelect {
                dst: V[0],
                cond: V[1],
                if_true: V[2],
                if_false: V[3],
                src_op: SRC_OP_NONE,
            },
            &[F],
            &[I, F, F],
        );
    }

    /// Addresses are integers; only the value crosses into the float file.
    #[test]
    fn float_memory_ops_keep_the_address_integer() {
        assert_signature(
            &VInst::FLoad32 {
                dst: V[0],
                base: V[1],
                offset: 0,
                src_op: SRC_OP_NONE,
            },
            &[F],
            &[I],
        );
        assert_signature(
            &VInst::FStore32 {
                src: V[0],
                base: V[1],
                offset: 0,
                src_op: SRC_OP_NONE,
            },
            &[],
            &[F, I],
        );
    }

    /// The boundary transfers, each reading one file and writing the other.
    /// A `Wfr` whose source was claimed Float would be a silent
    /// bit-reinterpretation: the allocator would hand it an FR, the emitter
    /// would read an FR, and an address register holding an IEEE pattern would
    /// never make it into the float file at all.
    #[test]
    fn transfers_cross_the_two_register_files() {
        assert_signature(
            &VInst::Wfr {
                dst: V[0],
                src: V[1],
                src_op: SRC_OP_NONE,
            },
            &[F],
            &[I],
        );
        assert_signature(
            &VInst::Rfr {
                dst: V[0],
                src: V[1],
                src_op: SRC_OP_NONE,
            },
            &[I],
            &[F],
        );
    }

    /// Conversion, not transfer: reads an integer *value*, writes a float.
    #[test]
    fn int_to_float_reads_an_integer() {
        for signed in [true, false] {
            assert_signature(
                &VInst::IToF {
                    dst: V[0],
                    src: V[1],
                    signed,
                    src_op: SRC_OP_NONE,
                },
                &[F],
                &[I],
            );
        }
    }

    /// `Call` and `Ret` have no float operands *by design* (M7 D1/D2): float
    /// values cross those boundaries in address registers, and lowering emits
    /// explicit `Rfr`/`Wfr` transfers to put them there. If this ever answers
    /// Float, the calling convention changed and `lpir_call_arg_target`'s
    /// float arm — which returns `None` — starts rejecting real code.
    #[test]
    fn calls_and_returns_carry_no_float_operands() {
        use crate::vinst::{SymbolId, VRegSlice};
        let pool = vec![VReg(0), VReg(1), VReg(2)];
        let call = VInst::Call {
            target: SymbolId(0),
            args: VRegSlice { start: 0, count: 2 },
            rets: VRegSlice { start: 2, count: 1 },
            callee_uses_sret: false,
            caller_passes_sret_ptr: false,
            caller_sret_vm_abi_swap: false,
            src_op: SRC_OP_NONE,
        };
        assert_eq!(def_class(&call, 0), I);
        for i in 0..2 {
            assert_eq!(use_class(&call, i), I);
        }
        let _ = &pool;

        let ret = VInst::Ret {
            vals: VRegSlice { start: 0, count: 2 },
            src_op: SRC_OP_NONE,
        };
        for i in 0..2 {
            assert_eq!(use_class(&ret, i), I);
        }
    }

    /// Class derivation over a mixed stream: the float defs land in the table,
    /// the integer ones stay implicit.
    #[test]
    fn a_mixed_function_records_only_its_float_vregs() {
        let insts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 0x3f80_0000u32 as i32,
                src_op: SRC_OP_NONE,
            },
            VInst::Wfr {
                dst: VReg(1),
                src: VReg(0),
                src_op: SRC_OP_NONE,
            },
            VInst::FAluRRR {
                op: FAluOp::Mul,
                dst: VReg(2),
                src1: VReg(1),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
            VInst::Rfr {
                dst: VReg(3),
                src: VReg(2),
                src_op: SRC_OP_NONE,
            },
        ];
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi());
        assert_eq!(classes.of(VReg(0)), I, "the raw bit pattern is an integer");
        assert_eq!(classes.of(VReg(1)), F);
        assert_eq!(classes.of(VReg(2)), F);
        assert_eq!(classes.of(VReg(3)), I, "back out to an address register");
    }
}
