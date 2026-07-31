//! The floating-point coprocessor's **data-movement** half, plus the Boolean
//! register file and the narrow special/user-register window M6 needs.
//!
//! Nothing here computes a float value. Every instruction in this file moves
//! bits: FR↔AR transfers, FP loads and stores, `mov.s`, `CPENABLE`/`BR`/`FCR`/
//! `FSR` access, and the Boolean branches and integer conditional moves that
//! make a compare observable. The arithmetic — and the explicit policy layer
//! that stands between it and Rust's `f32` — lives in
//! [`super::float_math`](super::float_math).
//!
//! The split is deliberate and worth keeping visible: a mistake in this file is
//! *structural* (state in the wrong place, gating modeled as always-on, an
//! invisible register write), and a mistake in the other is *numeric*. They
//! review differently.
//!
//! # `CPENABLE` is modeled, not assumed
//!
//! Every coprocessor-0 instruction checks `CPENABLE` bit 0 first and raises
//! [`EXC_COPROCESSOR0_DISABLED`] if it is clear. `Cpu::new()` starts it clear —
//! the architectural reset value — so a payload that forgets to arm the
//! coprocessor faults on the host rather than on a board. (On the desk S3 the
//! coprocessor arrives *already armed* under the esp-hal boot chain, with
//! provenance unpinned; M6 P1 measured that and M7 arms it defensively anyway.)
//!
//! Three groups are deliberately **not** gated, because they are not
//! coprocessor-0 instructions:
//!
//! - `rsr.cpenable` / `wsr.cpenable` / `xsr.cpenable` — the gate itself. Gating
//!   it would make the coprocessor impossible to arm.
//! - `rsr.br` / `wsr.br` / `xsr.br` — the Boolean register file is the Boolean
//!   *core* option, not part of the FPU.
//! - `bt` / `bf` / `movt` / `movf` — likewise Boolean-option instructions that
//!   read BR and write an AR or the PC. They touch no FR.
//!
//! `rur.fcr` / `wur.fcr` / `rur.fsr` / `wur.fsr` *are* gated: FCR and FSR are
//! the coprocessor's own user registers.
//!
//! Semantics come from the Xtensa ISA Reference Manual; no QEMU, binutils, or
//! GCC source was read or adapted (see
//! `docs/adr/2026-07-29-license-provenance-discipline.md`).

use lp_xt_inst::{FpLsiOp, FpLsxOp, FpRrOp, Inst, SpecialReg, SrOp, UrOp, UserReg};

use crate::emu::{Emulator, Flow};
use crate::error::{EXC_COPROCESSOR0_DISABLED, Trap, TrapKind};
use crate::trace::{TraceEvent, Tracer};

impl Emulator {
    /// Execute one FP / Boolean / special-register instruction.
    pub(super) fn exec_float(
        &mut self,
        inst: &Inst,
        pc: u32,
        tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        match *inst {
            // --- FR ↔ AR transfers: pure bit moves, no interpretation ---
            Inst::Rfr(ar, fs) => {
                self.require_fpu()?;
                let bits = self.rfreg(fs.num());
                self.wreg(ar.num(), bits, tracer);
            }
            Inst::Wfr(fr, ars) => {
                self.require_fpu()?;
                let bits = self.rreg(ars.num());
                self.wfreg(fr.num(), bits, tracer);
            }

            // --- `mov.s`: a raw bit copy. It must NOT canonicalize a NaN. ---
            Inst::FpRr(FpRrOp::MovS, fr, fs) => {
                self.require_fpu()?;
                let bits = self.rfreg(fs.num());
                self.wfreg(fr.num(), bits, tracer);
            }

            // --- FP load / store ---
            Inst::FpLsi(op, ft, ars, off) => {
                self.require_fpu()?;
                // The `p` (auto-update) forms are *pre*-increment: the effective
                // address is `AR[s] + offset` and `AR[s]` is then set to that
                // same address — not to the pre-increment base.
                let addr = self.rreg(ars.num()).wrapping_add(off);
                match op {
                    FpLsiOp::Lsi | FpLsiOp::Lsip => {
                        let v = self.mem.read_u32(addr)?;
                        self.wfreg(ft.num(), v, tracer);
                    }
                    FpLsiOp::Ssi | FpLsiOp::Ssip => {
                        let v = self.rfreg(ft.num());
                        self.mem.write_u32(addr, v)?;
                        tracer.event(TraceEvent::MemWrite {
                            addr,
                            value: v,
                            nbytes: 4,
                        });
                    }
                }
                if matches!(op, FpLsiOp::Lsip | FpLsiOp::Ssip) {
                    self.wreg(ars.num(), addr, tracer);
                }
            }
            Inst::FpLsx(op, fr, ars, at) => {
                self.require_fpu()?;
                let addr = self.rreg(ars.num()).wrapping_add(self.rreg(at.num()));
                match op {
                    FpLsxOp::Lsx | FpLsxOp::Lsxp => {
                        let v = self.mem.read_u32(addr)?;
                        self.wfreg(fr.num(), v, tracer);
                    }
                    FpLsxOp::Ssx | FpLsxOp::Ssxp => {
                        let v = self.rfreg(fr.num());
                        self.mem.write_u32(addr, v)?;
                        tracer.event(TraceEvent::MemWrite {
                            addr,
                            value: v,
                            nbytes: 4,
                        });
                    }
                }
                if matches!(op, FpLsxOp::Lsxp | FpLsxOp::Ssxp) {
                    self.wreg(ars.num(), addr, tracer);
                }
            }

            // --- special registers: BR and CPENABLE (neither is gated) ---
            Inst::Sr(op, sreg, at) => {
                let old = match sreg {
                    SpecialReg::Br => u32::from(self.cpu.br),
                    SpecialReg::Cpenable => self.cpu.cpenable,
                };
                let new = self.rreg(at.num());
                match op {
                    SrOp::Rsr => self.wreg(at.num(), old, tracer),
                    SrOp::Wsr => self.write_special(sreg, new, tracer),
                    SrOp::Xsr => {
                        self.write_special(sreg, new, tracer);
                        self.wreg(at.num(), old, tracer);
                    }
                }
            }

            // --- user registers: FCR and FSR (coprocessor 0 — gated) ---
            Inst::Ur(op, ureg, at) => {
                self.require_fpu()?;
                match op {
                    UrOp::Rur => {
                        let v = match ureg {
                            UserReg::Fcr => self.cpu.fcr,
                            UserReg::Fsr => self.cpu.fsr,
                        };
                        self.wreg(at.num(), v, tracer);
                    }
                    UrOp::Wur => {
                        let v = self.rreg(at.num());
                        match ureg {
                            // A write is the only thing that CLEARS FSR: the
                            // flags are sticky otherwise (measured, M6 P1).
                            UserReg::Fsr => self.cpu.fsr = v,
                            UserReg::Fcr => self.cpu.fcr = v,
                        }
                    }
                }
            }

            // --- Boolean-option: conditional AR move ---
            Inst::MovBool(want_set, ar, ars, bt) => {
                if self.cpu.b(bt.num()) == want_set {
                    let v = self.rreg(ars.num());
                    self.wreg(ar.num(), v, tracer);
                }
            }

            // --- Boolean-option: conditional branch ---
            // Like every Xtensa PC-relative branch the target is `PC + 4 + off`,
            // independent of instruction width.
            Inst::BranchBool(want_set, bs, off) => {
                if self.cpu.b(bs.num()) == want_set {
                    let target = pc.wrapping_add(4).wrapping_add(off as u32);
                    return Ok(Flow::Jump(target));
                }
            }

            // Everything that computes a float value.
            _ => return self.exec_float_math(inst, tracer),
        }
        Ok(Flow::Next)
    }

    /// Trap unless coprocessor 0 is enabled. `pc` is left 0 for the run loop to
    /// fill in, matching the other executors.
    pub(super) fn require_fpu(&self) -> Result<(), Trap> {
        if self.cpu.fpu_enabled() {
            Ok(())
        } else {
            Err(Trap {
                kind: TrapKind::Exception,
                cause: EXC_COPROCESSOR0_DISABLED,
                pc: 0,
                vaddr: 0,
            })
        }
    }

    fn write_special(&mut self, sreg: SpecialReg, v: u32, tracer: &mut dyn Tracer) {
        match sreg {
            SpecialReg::Br => {
                let new = v as u16;
                // Emit one event per changed bit so a bulk BR restore is as
                // visible in a trace as an individual compare result.
                let changed = new ^ self.cpu.br;
                for i in 0..16u8 {
                    if changed >> i & 1 != 0 {
                        self.wbreg(i, new >> i & 1 != 0, tracer);
                    }
                }
            }
            SpecialReg::Cpenable => self.cpu.cpenable = v,
        }
    }
}


#[cfg(test)]
mod tests {
    use lp_xt_inst::{
        BReg, FReg, FpLsiOp, FpRrOp, Inst, Reg, SpecialReg, SrOp, UrOp, UserReg, encode,
    };

    use crate::cpu::CPENABLE_FPU;
    use crate::emu::{Emulator, Flow, RunOutcome};
    use crate::error::{EXC_COPROCESSOR0_DISABLED, Trap, TrapKind};
    use crate::memory::EXC_LOAD_STORE_ERROR;
    use crate::trace::{NoopTracer, TextTracer};

    /// A straight-line FP payload plus the register state it starts from.
    ///
    /// Straight-line rather than through [`Emulator::run`] because these cases
    /// test the FP unit, not the windowed ABI: an `entry`/`retw` wrapper would
    /// add its own failure modes to every one of them. (One test below *does*
    /// go through the run loop, to prove the trap's `pc` gets filled in.)
    struct Payload {
        insts: Vec<Inst>,
        arm: bool,
        ars: Vec<(u8, u32)>,
    }

    impl Payload {
        fn new(insts: &[Inst]) -> Payload {
            Payload {
                insts: insts.to_vec(),
                arm: true,
                ars: Vec::new(),
            }
        }
        /// Leave `CPENABLE` at its architectural reset (coprocessor disabled).
        fn unarmed(mut self) -> Payload {
            self.arm = false;
            self
        }
        fn with_a(mut self, i: u8, v: u32) -> Payload {
            self.ars.push((i, v));
            self
        }
        fn run(self) -> (Emulator, Result<(), Trap>) {
            let mut t = NoopTracer;
            self.run_traced(&mut t)
        }
        fn run_traced(self, tracer: &mut dyn crate::trace::Tracer) -> (Emulator, Result<(), Trap>) {
            let mut code = Vec::new();
            for i in &self.insts {
                code.extend_from_slice(&encode(i));
            }
            let mut emu = Emulator::new();
            let base = emu.profile.code_ibus_base();
            emu.mem.load_bytes(base, &code);
            if self.arm {
                emu.cpu.cpenable = CPENABLE_FPU;
            }
            for (i, v) in &self.ars {
                emu.cpu.set_a(*i, *v);
            }
            emu.cpu.pc = base;
            let end = base + code.len() as u32;
            let mut res = Ok(());
            while emu.cpu.pc < end {
                let pc = emu.cpu.pc;
                let mut bytes = [0u8; 3];
                let got = emu.mem.fetch(pc, &mut bytes).expect("fetch");
                let (inst, len) = lp_xt_inst::decode(&bytes[..got]).expect("decode");
                match emu.execute(&inst, pc, tracer) {
                    Ok(Flow::Next) => emu.cpu.pc = pc + len as u32,
                    Ok(Flow::Jump(a)) => emu.cpu.pc = a,
                    Ok(Flow::Syscall) => unreachable!("no syscall in these payloads"),
                    Err(mut trap) => {
                        if trap.pc == 0 {
                            trap.pc = pc;
                        }
                        res = Err(trap);
                        break;
                    }
                }
            }
            (emu, res)
        }
    }

    /// The one behavior in this file that is about correctness rather than
    /// plumbing: the same payload must fault with the coprocessor disabled and
    /// run with it enabled.
    #[test]
    fn cpenable_gates_fp_and_raises_exccause_32() {
        let insts = [
            Inst::Wfr(FReg::new(0), Reg::new(2)),
            Inst::Rfr(Reg::new(3), FReg::new(0)),
        ];

        let (_, disabled) = Payload::new(&insts).unarmed().with_a(2, 0x1234).run();
        let trap = disabled.expect_err("FP with CPENABLE clear must trap");
        assert_eq!(trap.cause, EXC_COPROCESSOR0_DISABLED);
        assert_eq!(trap.kind, TrapKind::Exception);

        let (emu, armed) = Payload::new(&insts).with_a(2, 0x1234).run();
        armed.expect("FP with CPENABLE set must run");
        assert_eq!(emu.cpu.a(3), 0x1234);
    }

    /// `wsr.cpenable` must not itself be gated, or the coprocessor could never
    /// be armed from guest code.
    #[test]
    fn cpenable_write_is_not_itself_gated() {
        let (emu, res) = Payload::new(&[
            Inst::Sr(SrOp::Wsr, SpecialReg::Cpenable, Reg::new(2)),
            Inst::Wfr(FReg::new(1), Reg::new(4)),
            Inst::Rfr(Reg::new(5), FReg::new(1)),
        ])
        .unarmed()
        .with_a(2, 1)
        .with_a(4, 0x55)
        .run();
        res.expect("arming from guest code must work");
        assert_eq!(emu.cpu.cpenable & CPENABLE_FPU, CPENABLE_FPU);
        assert_eq!(emu.cpu.a(5), 0x55);
    }

    /// If FP state were stored as `f32` anywhere, this fails: a signalling NaN
    /// payload survives `wfr` -> `mov.s` -> `rfr` bit for bit.
    #[test]
    fn nan_payload_survives_a_move_bit_exact() {
        // sNaN: exponent all ones, quiet bit CLEAR, payload non-zero.
        const SNAN: u32 = 0x7F80_1234;
        let (emu, res) = Payload::new(&[
            Inst::Wfr(FReg::new(2), Reg::new(2)),
            Inst::FpRr(FpRrOp::MovS, FReg::new(3), FReg::new(2)),
            Inst::Rfr(Reg::new(6), FReg::new(3)),
        ])
        .with_a(2, SNAN)
        .run();
        res.expect("no trap");
        assert_eq!(emu.cpu.f(2), SNAN, "wfr must not canonicalize");
        assert_eq!(emu.cpu.f(3), SNAN, "mov.s must not canonicalize");
        assert_eq!(emu.cpu.a(6), SNAN, "rfr must not canonicalize");
    }

    #[test]
    fn immediate_load_store_round_trips_and_the_p_form_updates_its_base() {
        let sp = Emulator::new().profile.initial_sp() - 64;
        let (emu, res) = Payload::new(&[
            Inst::Wfr(FReg::new(0), Reg::new(2)),
            // plain form: a3 must be left alone
            Inst::FpLsi(FpLsiOp::Ssi, FReg::new(0), Reg::new(3), 8),
            Inst::FpLsi(FpLsiOp::Lsi, FReg::new(1), Reg::new(3), 8),
            // p form: loads from a4 + 8 AND writes a4 + 8 back to a4
            Inst::FpLsi(FpLsiOp::Lsip, FReg::new(2), Reg::new(4), 8),
        ])
        .with_a(2, 0xDEAD_BEEF)
        .with_a(3, sp)
        .with_a(4, sp)
        .run();
        res.expect("no trap");
        assert_eq!(emu.cpu.f(1), 0xDEAD_BEEF, "store/load round trip");
        assert_eq!(emu.cpu.f(2), 0xDEAD_BEEF, "lsip loads from base + offset");
        assert_eq!(emu.cpu.a(3), sp, "plain form leaves the base alone");
        assert_eq!(emu.cpu.a(4), sp + 8, "p form writes back base + offset");
    }

    #[test]
    fn fp_load_from_unmapped_memory_raises_load_store_error() {
        let (_, res) = Payload::new(&[Inst::FpLsi(FpLsiOp::Lsi, FReg::new(0), Reg::new(3), 0)])
            .with_a(3, 0x0000_1000) // unmapped on the S3 profile
            .run();
        assert_eq!(res.expect_err("must fault").cause, EXC_LOAD_STORE_ERROR);
    }

    #[test]
    fn boolean_branches_and_conditional_moves_read_br() {
        // wsr.br from a2 = 0b101 -> b0 and b2 set, b1 clear.
        let (emu, res) = Payload::new(&[
            Inst::Sr(SrOp::Wsr, SpecialReg::Br, Reg::new(2)),
            // movt a5, a6, b2  (b2 set -> moves)
            Inst::MovBool(true, Reg::new(5), Reg::new(6), BReg::new(2)),
            // movf a7, a6, b2  (b2 set -> does NOT move)
            Inst::MovBool(false, Reg::new(7), Reg::new(6), BReg::new(2)),
        ])
        .with_a(2, 0b101)
        .with_a(6, 0x77)
        .with_a(7, 0x11)
        .run();
        res.expect("no trap");
        assert!(emu.cpu.b(0) && !emu.cpu.b(1) && emu.cpu.b(2));
        assert_eq!(emu.cpu.a(5), 0x77, "movt on a set bit moves");
        assert_eq!(emu.cpu.a(7), 0x11, "movf on a set bit does not move");
    }

    /// Branch targets are `PC + 4 + offset`, as for every Xtensa PC-relative
    /// branch, independent of instruction width.
    #[test]
    fn bt_and_bf_branch_on_the_boolean_file() {
        let mut emu = Emulator::new();
        let mut t = NoopTracer;
        emu.cpu.set_b(0, true);
        let pc = 0x4000_0100;
        assert_eq!(
            emu.execute(&Inst::BranchBool(true, BReg::new(0), 12), pc, &mut t)
                .unwrap(),
            Flow::Jump(pc + 4 + 12),
            "bt on a set bit is taken"
        );
        assert_eq!(
            emu.execute(&Inst::BranchBool(false, BReg::new(0), 12), pc, &mut t)
                .unwrap(),
            Flow::Next,
            "bf on a set bit is not taken"
        );
        assert_eq!(
            emu.execute(&Inst::BranchBool(true, BReg::new(0), -8), pc, &mut t)
                .unwrap(),
            Flow::Jump(pc + 4 - 8),
            "negative offsets branch backwards"
        );
    }

    #[test]
    fn xsr_br_exchanges_and_rsr_reads_back() {
        let (emu, res) = Payload::new(&[
            Inst::Sr(SrOp::Wsr, SpecialReg::Br, Reg::new(2)),
            Inst::Sr(SrOp::Xsr, SpecialReg::Br, Reg::new(3)),
            Inst::Sr(SrOp::Rsr, SpecialReg::Br, Reg::new(4)),
        ])
        .with_a(2, 0xF0F0)
        .with_a(3, 0x0A0A)
        .run();
        res.expect("no trap");
        assert_eq!(emu.cpu.br, 0x0A0A, "xsr installed the new value");
        assert_eq!(emu.cpu.a(3), 0xF0F0, "xsr returned the old value");
        assert_eq!(emu.cpu.a(4), 0x0A0A, "rsr reads all 16 bits at once");
    }

    #[test]
    fn fcr_and_fsr_are_gated_and_fsr_is_sticky() {
        let (_, res) = Payload::new(&[Inst::Ur(UrOp::Rur, UserReg::Fsr, Reg::new(2))])
            .unarmed()
            .run();
        assert_eq!(
            res.expect_err("FCR/FSR are coprocessor-0 user registers")
                .cause,
            EXC_COPROCESSOR0_DISABLED
        );

        // Reset values, measured on an ESP32-S3 (M6 P1, 2026-07-31): both zero,
        // FCR = 0 meaning round-to-nearest-even.
        let mut emu = Emulator::new();
        emu.cpu.cpenable = CPENABLE_FPU;
        let mut t = NoopTracer;
        let pc = 0x4000_0100;
        emu.execute(&Inst::Ur(UrOp::Rur, UserReg::Fcr, Reg::new(3)), pc, &mut t)
            .unwrap();
        assert_eq!(emu.cpu.a(3), 0, "FCR resets to 0 (round-to-nearest-even)");

        emu.cpu.or_fsr(0x400);
        emu.cpu.or_fsr(0x001);
        assert_eq!(emu.cpu.fsr, 0x401, "flags accumulate, they do not replace");
        emu.execute(&Inst::Ur(UrOp::Rur, UserReg::Fsr, Reg::new(4)), pc, &mut t)
            .unwrap();
        assert_eq!(emu.cpu.a(4), 0x401);
        emu.cpu.set_a(5, 0);
        emu.execute(&Inst::Ur(UrOp::Wur, UserReg::Fsr, Reg::new(5)), pc, &mut t)
            .unwrap();
        assert_eq!(emu.cpu.fsr, 0, "a write is the only thing that clears FSR");
    }

    /// The FR file must not rotate with the register window. This asymmetry --
    /// AR preservation is free, FR preservation is not -- is what M7's frame
    /// layout has to answer for.
    #[test]
    fn the_fr_file_is_flat_across_a_window_rotation() {
        let mut emu = Emulator::new();
        emu.cpu.set_f(7, 0xCAFE_F00D);
        emu.cpu.set_a(7, 0x1111_1111);
        emu.cpu.window_base = 3;
        assert_eq!(emu.cpu.f(7), 0xCAFE_F00D, "f7 is unaffected by WindowBase");
        assert_ne!(
            emu.cpu.a(7),
            0x1111_1111,
            "control: a7 IS affected, else this test proves nothing"
        );
    }

    /// FP writes and compare-visible boolean writes reach the tracer, so P6 can
    /// bisect a numeric divergence off an intermediate state.
    #[test]
    fn fp_and_boolean_writes_are_traced() {
        let mut tracer = TextTracer::new();
        let (_, res) = Payload::new(&[
            Inst::Wfr(FReg::new(4), Reg::new(2)),
            Inst::Sr(SrOp::Wsr, SpecialReg::Br, Reg::new(3)),
        ])
        .with_a(2, 0x3F80_0000)
        .with_a(3, 0b10)
        .run_traced(&mut tracer);
        res.expect("no trap");
        let dump = tracer.dump();
        assert!(dump.contains("f4 <- 0x3f800000"), "missing FRegWrite: {dump}");
        assert!(dump.contains("b1 <- 1"), "missing BRegWrite: {dump}");
    }

    #[test]
    fn dump_state_shows_the_fp_block() {
        let mut emu = Emulator::new();
        emu.cpu.set_f(0, 0x7F80_1234);
        let s = emu.dump_state();
        assert!(s.contains("FPU DISABLED"), "{s}");
        assert!(s.contains("0x7f801234"), "raw bits are printed: {s}");
        emu.cpu.cpenable = CPENABLE_FPU;
        assert!(emu.dump_state().contains("FPU armed"));
    }

    /// Through the real run loop, so the trap's `pc` is filled in.
    #[test]
    fn an_unarmed_windowed_run_reports_a_coprocessor_trap() {
        let code = encode(&Inst::Wfr(FReg::new(0), Reg::new(2)));
        let mut emu = Emulator::new();
        match emu.run(&code, 0, 0) {
            RunOutcome::Trap(t) => {
                assert_eq!(t.cause, EXC_COPROCESSOR0_DISABLED);
                assert_ne!(t.pc, 0, "the run loop fills in the faulting pc");
            }
            other => panic!("expected a trap, got {other:?}"),
        }
    }
}
