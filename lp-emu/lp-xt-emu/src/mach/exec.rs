//! The instructions the hart executes itself: everything privileged, every
//! special/user-register access, and the families the user-mode executors
//! refuse as machine-mode-only (`loop`, `s32c1i`, `l32e`/`s32e`, `rotw`,
//! `break`, MAC16, `clamps`, the Boolean ops, the TLB ops).
//!
//! Semantics are the Xtensa ISA Reference Manual's, cited per instruction.
//! No QEMU/binutils/GCC source (AGENTS.md).

use lp_emu_core::{Bus, InstClass};
use lp_xt_inst::{
    AtomicLsOp, BoolAllOp, BoolOp, Inst, LoopOp, NullaryOp, RfOp, SpecialReg, SrOp, TlbOp, UrOp,
    UserReg, WindowLsOp,
};

use super::XtHart;
use super::interrupt::InterruptUnit;
use super::sr::{PS_EXCM, PS_INTLEVEL_MASK, PS_OWB_MASK, PS_OWB_SHIFT};
use super::trap::{DEBUGLEVEL, NMI_LEVEL, cause};
use super::window;
use crate::emu::Flow;
use crate::error::{EXC_COPROCESSOR0_DISABLED, Trap, TrapKind};
use crate::executor::inst_class;
use crate::memory::XtAccess;
use crate::trace::{TraceEvent, Tracer};

/// What a hart-owned instruction did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Priv {
    /// Retire normally; `poll` marks an instruction at poll point (b).
    Retire {
        flow: Flow,
        class: InstClass,
        poll: bool,
    },
    /// `waiti` retired: park.
    Waiti,
    /// `break` / `break.n`: hand the instruction to the machine.
    Break { narrow: bool },
}

/// Does the hart execute this instruction itself (rather than the shared
/// executors)?
#[inline]
pub(super) fn is_hart_owned(inst: &Inst) -> bool {
    matches!(
        inst,
        Inst::Sr(..)
            | Inst::Ur(..)
            | Inst::Rf(_)
            | Inst::Rfi(_)
            | Inst::Rsil(..)
            | Inst::Waiti(_)
            | Inst::Rotw(_)
            | Inst::Break(..)
            | Inst::BreakN(_)
            | Inst::Loop(..)
            | Inst::AtomicLs(..)
            | Inst::WindowLs(..)
            | Inst::Nullary(NullaryOp::Syscall)
            // `isync` has no architectural effect and the shared executor
            // handles it as the barrier it is; the hart claims it only to
            // raise the translation-invalidating event. See
            // `super::translated`'s "three invalidation events".
            | Inst::Nullary(NullaryOp::Isync)
            | Inst::Tlb(..)
            | Inst::TlbInv(..)
            | Inst::ExtReg(..)
            | Inst::Mac(..)
            | Inst::MacLd(..)
            | Inst::MacLoad(..)
            | Inst::Clamps(..)
            | Inst::BoolLogic(..)
            | Inst::BoolAll(..)
    )
}

/// An instruction the emulator declines rather than the guest executing
/// something genuinely illegal — the strict-stop set. External registers
/// have no model here; an `rsr`/`wsr` of a register number outside the SR
/// table never decodes (`lp-xt-inst` refuses it), so it arrives as an
/// `Unsupported` word instead.
#[inline]
pub(super) fn is_unimplemented(inst: &Inst) -> bool {
    matches!(inst, Inst::ExtReg(..))
}

fn illegal() -> Trap {
    Trap {
        kind: TrapKind::Exception,
        cause: cause::ILLEGAL_INSTRUCTION,
        pc: 0,
        vaddr: 0,
    }
}

fn retire(flow: Flow, class: InstClass, poll: bool) -> Result<Priv, Trap> {
    Ok(Priv::Retire { flow, class, poll })
}

impl<B: Bus> XtHart<B> {
    /// Write windowed register `a{i}` with a trace event, as the executors'
    /// `wreg` does.
    #[inline]
    fn wreg<T: Tracer + ?Sized>(&mut self, i: u8, v: u32, tracer: &mut T) {
        let phys = self.cpu.set_a(i, v);
        tracer.event(TraceEvent::RegWrite {
            index: i,
            phys,
            value: v,
        });
    }

    /// `wsr.br`: one event per changed bit, as user mode emits.
    fn write_br<T: Tracer + ?Sized>(&mut self, v: u32, tracer: &mut T) {
        let new = v as u16;
        let changed = new ^ self.cpu.br;
        self.cpu.br = new;
        for i in 0..16u8 {
            if changed >> i & 1 != 0 {
                tracer.event(TraceEvent::BRegWrite {
                    index: i,
                    value: new >> i & 1 != 0,
                });
            }
        }
    }

    pub(super) fn exec_priv<T: Tracer + ?Sized>(
        &mut self,
        bus: &mut B,
        inst: &Inst,
        pc: u32,
        len: u32,
        tracer: &mut T,
    ) -> Result<Priv, Trap> {
        let class = |flow: &Flow| inst_class(inst, flow);
        match *inst {
            // --- special registers (RM §5.3, per-register tables) ---
            Inst::Sr(op, reg, at) => {
                let t = at.num();
                let new = self.cpu.a(t);
                let poll = match op {
                    SrOp::Rsr => {
                        let old = self.sr_read(reg).ok_or_else(illegal)?;
                        self.wreg(t, old, tracer);
                        false
                    }
                    SrOp::Wsr => self.sr_write(bus, reg, new, tracer)?,
                    SrOp::Xsr => {
                        let old = self.sr_read(reg).ok_or_else(illegal)?;
                        let poll = self.sr_write(bus, reg, new, tracer)?;
                        self.wreg(t, old, tracer);
                        poll
                    }
                };
                retire(Flow::Next, InstClass::System, poll)
            }

            // --- user registers ---
            Inst::Ur(op, reg, at) => {
                let t = at.num();
                // FCR/FSR are coprocessor-0 state: gated like every FP
                // instruction, as the user-mode executor gates them.
                if matches!(reg, UserReg::Fcr | UserReg::Fsr) && !self.cpu.fpu_enabled() {
                    return Err(Trap {
                        kind: TrapKind::Exception,
                        cause: EXC_COPROCESSOR0_DISABLED,
                        pc: 0,
                        vaddr: 0,
                    });
                }
                match op {
                    UrOp::Rur => {
                        let v = match reg {
                            UserReg::Threadptr => self.sr.threadptr,
                            UserReg::Fcr => self.cpu.fcr,
                            UserReg::Fsr => self.cpu.fsr,
                            UserReg::Expstate => self.sr.expstate,
                            UserReg::F64rLo => self.sr.f64r_lo,
                            UserReg::F64rHi => self.sr.f64r_hi,
                            UserReg::F64s => self.sr.f64s,
                        };
                        self.wreg(t, v, tracer);
                    }
                    UrOp::Wur => {
                        let v = self.cpu.a(t);
                        match reg {
                            UserReg::Threadptr => self.sr.threadptr = v,
                            UserReg::Fcr => self.cpu.fcr = v,
                            // A write is the only thing that clears FSR.
                            UserReg::Fsr => self.cpu.fsr = v,
                            UserReg::Expstate => self.sr.expstate = v,
                            UserReg::F64rLo => self.sr.f64r_lo = v,
                            UserReg::F64rHi => self.sr.f64r_hi = v,
                            UserReg::F64s => self.sr.f64s = v,
                        }
                    }
                }
                retire(Flow::Next, InstClass::System, false)
            }

            // --- exception returns (the RFE/RFDE/RFWO/RFWU pages) ---
            Inst::Rf(op) => {
                let target = match op {
                    RfOp::Rfe => {
                        self.ps &= !PS_EXCM;
                        self.sr.epc[1]
                    }
                    // PS.EXCM is *not* cleared; DEPC is the return address.
                    RfOp::Rfde => self.sr.depc,
                    RfOp::Rfwo | RfOp::Rfwu => {
                        self.ps &= !PS_EXCM;
                        let owb = ((self.ps & PS_OWB_MASK) >> PS_OWB_SHIFT) as u8;
                        window::rfw(&mut self.cpu, owb, op == RfOp::Rfwu);
                        tracer.event(TraceEvent::WindowRotate {
                            what: if op == RfOp::Rfwu { "rfwu" } else { "rfwo" },
                            old_base: owb,
                            new_base: self.cpu.window_base,
                            window_start: self.cpu.window_start,
                        });
                        self.sr.epc[1]
                    }
                };
                // (b): rfe/rfwo/rfwu cleared EXCM; rfde changed nothing, and
                // polling once more costs nothing.
                retire(Flow::Jump(target), InstClass::System, true)
            }

            // `rfi level`: `PS <- EPS[level]`, `PC <- EPC[level]`. Levels 0,
            // 1 and above NMI are undefined by the RM; raising is the loud
            // reading.
            Inst::Rfi(level) => {
                if !(2..=NMI_LEVEL).contains(&level) {
                    return Err(illegal());
                }
                let l = usize::from(level);
                let target = self.sr.epc[l];
                let ps = self.sr.eps[l];
                self.set_ps_raw(ps);
                retire(Flow::Jump(target), InstClass::System, true)
            }

            // `rsil at, level`: `AR[t] <- PS; PS.INTLEVEL <- level`.
            Inst::Rsil(at, level) => {
                let old = self.ps();
                self.wreg(at.num(), old, tracer);
                self.ps = (self.ps & !PS_INTLEVEL_MASK) | u32::from(level & 0xF);
                retire(Flow::Next, InstClass::System, true)
            }

            // `waiti level`: `PS.INTLEVEL <- level`, then park. The slice
            // loop advances pc past it (RM: EPC will hold the instruction
            // following WAITI).
            Inst::Waiti(level) => {
                self.ps = (self.ps & !PS_INTLEVEL_MASK) | u32::from(level & 0xF);
                Ok(Priv::Waiti)
            }

            Inst::Rotw(imm) => {
                let old = self.cpu.window_base;
                window::rotw(&mut self.cpu, imm);
                tracer.event(TraceEvent::WindowRotate {
                    what: "rotw",
                    old_base: old,
                    new_base: self.cpu.window_base,
                    window_start: self.cpu.window_start,
                });
                retire(Flow::Next, InstClass::System, false)
            }

            Inst::Break(..) => Ok(Priv::Break { narrow: false }),
            Inst::BreakN(_) => Ok(Priv::Break { narrow: true }),

            // --- `isync`: the Xtensa `fence.i` ---
            //
            // Architecturally a pipeline barrier this model does not need, and
            // the shared executor retires it as a no-op with exactly this
            // class and flow. The hart claims it so that the *translation*
            // event it carries is raised: the guest has just published
            // instructions. With no translated core installed nothing happens
            // beyond a counter, and the trace is byte-identical.
            Inst::Nullary(NullaryOp::Isync) => {
                self.on_isync();
                retire(Flow::Next, InstClass::System, false)
            }

            // --- zero-overhead loops (the LOOP/LOOPNEZ/LOOPGTZ pages) ---
            //
            // `LCOUNT <- AR[s] - 1`, `LBEG <- pc + 3`, `LEND <- pc + 4 +
            // imm8`; the guarded forms skip the body outright. The registers
            // are loaded even when the loop is skipped. Note for P5: a write
            // to LBEG/LEND/LCOUNT — here or by `wsr` — is a translation-
            // invalidating event; nothing may cache them across one.
            Inst::Loop(op, r#as, imm8) => {
                let count = self.cpu.a(r#as.num());
                let lend = lp_xt_inst::disasm::loop_end(pc, imm8);
                self.sr.lcount = count.wrapping_sub(1);
                self.sr.lbeg = pc.wrapping_add(3);
                self.sr.lend = lend;
                let skip = match op {
                    LoopOp::Loop => false,
                    LoopOp::Loopnez => count == 0,
                    LoopOp::Loopgtz => (count as i32) <= 0,
                };
                let flow = if skip { Flow::Jump(lend) } else { Flow::Next };
                retire(flow, InstClass::System, false)
            }

            // --- the synchronising accesses ---
            Inst::AtomicLs(op, at, r#as, off) => {
                let addr = self.cpu.a(r#as.num()).wrapping_add(off);
                match op {
                    AtomicLsOp::L32ai => {
                        let v = bus.read_u32(addr)?;
                        self.wreg(at.num(), v, tracer);
                        retire(Flow::Next, InstClass::Load, false)
                    }
                    AtomicLsOp::S32ri => {
                        let v = self.cpu.a(at.num());
                        bus.write_u32(addr, v)?;
                        tracer.event(TraceEvent::MemWrite {
                            addr,
                            value: v,
                            nbytes: 4,
                        });
                        retire(Flow::Next, InstClass::Store, false)
                    }
                    // `if mem[addr] == SCOMPARE1 { mem[addr] = AR[t] };
                    // AR[t] = old` — the loaded value goes back to the
                    // register either way, which is how the guest's
                    // `critical-section` tells success from failure. Single
                    // hart, so the pair needs no real atomicity; the
                    // observable protocol is what matters.
                    AtomicLsOp::S32c1i => {
                        let old = bus.read_u32(addr)?;
                        if old == self.sr.scompare1 {
                            let v = self.cpu.a(at.num());
                            bus.write_u32(addr, v)?;
                            tracer.event(TraceEvent::MemWrite {
                                addr,
                                value: v,
                                nbytes: 4,
                            });
                        }
                        self.wreg(at.num(), old, tracer);
                        retire(Flow::Next, InstClass::Store, false)
                    }
                }
            }

            // --- the window handlers' accesses (L32E/S32E pages): L32I/S32I
            // with a negative offset; the ring semantics do not exist here.
            Inst::WindowLs(op, at, r#as, off) => {
                let addr = self.cpu.a(r#as.num()).wrapping_add(off as u32);
                match op {
                    WindowLsOp::L32e => {
                        let v = bus.read_u32(addr)?;
                        self.wreg(at.num(), v, tracer);
                        retire(Flow::Next, InstClass::Load, false)
                    }
                    WindowLsOp::S32e => {
                        let v = self.cpu.a(at.num());
                        bus.write_u32(addr, v)?;
                        tracer.event(TraceEvent::MemWrite {
                            addr,
                            value: v,
                            nbytes: 4,
                        });
                        retire(Flow::Next, InstClass::Store, false)
                    }
                }
            }

            // `syscall`: `SyscallCause` through the general vector. The
            // user-mode `SyscallHandler` is not involved in machine mode.
            Inst::Nullary(NullaryOp::Syscall) => Err(Trap {
                kind: TrapKind::Exception,
                cause: cause::SYSCALL,
                pc: 0,
                vaddr: 0,
            }),

            // --- region protection: accept and remember (RM §4.6.3.2) ---
            Inst::Tlb(op, at, r#as) => {
                let s = self.cpu.a(r#as.num());
                let region = (s >> 29) as usize;
                let t = at.num();
                match op {
                    TlbOp::Witlb => self.sr.itlb_attr[region] = (self.cpu.a(t) & 0xF) as u8,
                    TlbOp::Wdtlb => self.sr.dtlb_attr[region] = (self.cpu.a(t) & 0xF) as u8,
                    TlbOp::Ritlb1 => {
                        let v = (s & 0xE000_0000) | u32::from(self.sr.itlb_attr[region]);
                        self.wreg(t, v, tracer);
                    }
                    TlbOp::Rdtlb1 => {
                        let v = (s & 0xE000_0000) | u32::from(self.sr.dtlb_attr[region]);
                        self.wreg(t, v, tracer);
                    }
                    // "The read instructions return zero in the at register."
                    TlbOp::Ritlb0 | TlbOp::Rdtlb0 => self.wreg(t, 0, tracer),
                    // "The VPN is returned in the upper bits. The low bit is
                    // set because the probe always hits."
                    TlbOp::Pitlb | TlbOp::Pdtlb => self.wreg(t, (s & 0xE000_0000) | 1, tracer),
                }
                retire(Flow::Next, InstClass::System, false)
            }
            // "IITLB and IDTLB ... have no effect because the entries cannot
            // be removed."
            Inst::TlbInv(..) => retire(Flow::Next, InstClass::System, false),

            // External registers: no model. Loud (see `is_unimplemented`).
            Inst::ExtReg(..) => Err(illegal()),

            // --- MAC16 (ruling R4) ---
            Inst::Mac(op, half, src) => {
                let (x, y) = self.mac.operands(src, |r| self.cpu.a(r));
                self.mac.multiply(op, half, x, y);
                retire(Flow::Next, InstClass::Mul, false)
            }
            // `mula.<xy>.<half>.ldinc/lddec mw, as, mx, y`: multiply-
            // accumulate with the MR operands as they are *before* the load,
            // then the auto-incremented load into `mw`.
            Inst::MacLd(dec, half, mw, r#as, mx, y) => {
                let base = self.cpu.a(r#as.num());
                let addr = if dec {
                    base.wrapping_sub(4)
                } else {
                    base.wrapping_add(4)
                };
                let loaded = bus.read_u32(addr)?;
                let x = self.mac.mr[mx.num() as usize];
                let yv = self.mac.y_operand(y, |r| self.cpu.a(r));
                self.mac.multiply(lp_xt_inst::MacOp::Mula, half, x, yv);
                self.wreg(r#as.num(), addr, tracer);
                self.mac.mr[mw.num() as usize] = loaded;
                retire(Flow::Next, InstClass::Mul, false)
            }
            Inst::MacLoad(dec, mw, r#as) => {
                let base = self.cpu.a(r#as.num());
                let addr = if dec {
                    base.wrapping_sub(4)
                } else {
                    base.wrapping_add(4)
                };
                let loaded = bus.read_u32(addr)?;
                self.wreg(r#as.num(), addr, tracer);
                self.mac.mr[mw.num() as usize] = loaded;
                retire(Flow::Next, InstClass::Load, false)
            }

            // `clamps ar, as, imm`: `min(max(x, -2^imm), 2^imm - 1)` (the
            // CLAMPS page). `imm` is 7..=22 as decoded.
            Inst::Clamps(rd, rs, imm) => {
                let x = self.cpu.a(rs.num()) as i32;
                let hi = (1i32 << imm) - 1;
                let lo = -(1i32 << imm);
                let y = x.clamp(lo, hi);
                self.wreg(rd.num(), y as u32, tracer);
                retire(Flow::Next, InstClass::Alu, false)
            }

            // --- the Boolean option's logic and reductions ---
            Inst::BoolLogic(op, br, bs, bt) => {
                let s = self.cpu.b(bs.num());
                let t = self.cpu.b(bt.num());
                let v = match op {
                    BoolOp::Andb => s && t,
                    BoolOp::Andbc => s && !t,
                    BoolOp::Orb => s || t,
                    BoolOp::Orbc => s || !t,
                    BoolOp::Xorb => s ^ t,
                };
                self.cpu.set_b(br.num(), v);
                tracer.event(TraceEvent::BRegWrite {
                    index: br.num(),
                    value: v,
                });
                retire(Flow::Next, InstClass::Alu, false)
            }
            Inst::BoolAll(op, bt, bs) => {
                let (n, all) = match op {
                    BoolAllOp::Any4 => (4u8, false),
                    BoolAllOp::All4 => (4, true),
                    BoolAllOp::Any8 => (8, false),
                    BoolAllOp::All8 => (8, true),
                };
                let bits = (0..n).map(|i| self.cpu.b(bs.num().wrapping_add(i) & 0xF));
                let v = if all {
                    bits.fold(true, |a, b| a && b)
                } else {
                    bits.fold(false, |a, b| a || b)
                };
                self.cpu.set_b(bt.num(), v);
                tracer.event(TraceEvent::BRegWrite {
                    index: bt.num(),
                    value: v,
                });
                retire(Flow::Next, InstClass::Alu, false)
            }

            _ => unreachable!("exec_priv got {inst:?} (len {len})"),
        }
        .map(|p| match p {
            // The class the standalone mapping would give, so `inst_class`
            // and this file cannot drift (the executors' own guard).
            Priv::Retire {
                flow,
                class: c,
                poll,
            } => {
                debug_assert_eq!(
                    c,
                    class(&flow),
                    "exec_priv and inst_class disagree on {inst:?}"
                );
                Priv::Retire {
                    flow,
                    class: c,
                    poll,
                }
            }
            other => other,
        })
    }

    /// Read a special register, or `None` when the access is illegal (a
    /// write-only register, or one this hart does not keep).
    pub(super) fn sr_read(&self, reg: SpecialReg) -> Option<u32> {
        Some(match reg {
            SpecialReg::Lbeg => self.sr.lbeg,
            SpecialReg::Lend => self.sr.lend,
            SpecialReg::Lcount => self.sr.lcount,
            SpecialReg::Sar => self.cpu.sar,
            SpecialReg::Br => u32::from(self.cpu.br),
            SpecialReg::Litbase => self.sr.litbase,
            SpecialReg::Scompare1 => self.sr.scompare1,
            SpecialReg::Acclo => self.mac.acclo(),
            SpecialReg::Acchi => self.mac.acchi(),
            SpecialReg::M0 => self.mac.mr[0],
            SpecialReg::M1 => self.mac.mr[1],
            SpecialReg::M2 => self.mac.mr[2],
            SpecialReg::M3 => self.mac.mr[3],
            SpecialReg::WindowBase => u32::from(self.cpu.window_base),
            SpecialReg::WindowStart => u32::from(self.cpu.window_start),
            SpecialReg::Mmid => return None,
            SpecialReg::IbreakEnable => self.breaks.ibreakenable,
            SpecialReg::Memctl => self.sr.memctl,
            SpecialReg::Atomctl => self.sr.atomctl,
            SpecialReg::Ddr => self.sr.ddr,
            SpecialReg::Ibreaka0 => self.breaks.ibreaka[0],
            SpecialReg::Ibreaka1 => self.breaks.ibreaka[1],
            SpecialReg::Dbreaka0 => self.breaks.dbreaka[0],
            SpecialReg::Dbreaka1 => self.breaks.dbreaka[1],
            SpecialReg::Dbreakc0 => self.breaks.dbreakc[0],
            SpecialReg::Dbreakc1 => self.breaks.dbreakc[1],
            SpecialReg::Epc1 => self.sr.epc[1],
            SpecialReg::Epc2 => self.sr.epc[2],
            SpecialReg::Epc3 => self.sr.epc[3],
            SpecialReg::Epc4 => self.sr.epc[4],
            SpecialReg::Epc5 => self.sr.epc[5],
            SpecialReg::Epc6 => self.sr.epc[6],
            SpecialReg::Epc7 => self.sr.epc[7],
            SpecialReg::Depc => self.sr.depc,
            SpecialReg::Eps2 => self.sr.eps[2],
            SpecialReg::Eps3 => self.sr.eps[3],
            SpecialReg::Eps4 => self.sr.eps[4],
            SpecialReg::Eps5 => self.sr.eps[5],
            SpecialReg::Eps6 => self.sr.eps[6],
            SpecialReg::Eps7 => self.sr.eps[7],
            SpecialReg::Excsave1 => self.sr.excsave[1],
            SpecialReg::Excsave2 => self.sr.excsave[2],
            SpecialReg::Excsave3 => self.sr.excsave[3],
            SpecialReg::Excsave4 => self.sr.excsave[4],
            SpecialReg::Excsave5 => self.sr.excsave[5],
            SpecialReg::Excsave6 => self.sr.excsave[6],
            SpecialReg::Excsave7 => self.sr.excsave[7],
            SpecialReg::Cpenable => self.cpu.cpenable,
            SpecialReg::Interrupt => self.ints.pending(),
            SpecialReg::Intclear => return None,
            SpecialReg::Intenable => self.ints.intenable,
            SpecialReg::Ps => self.ps(),
            SpecialReg::Vecbase => self.sr.vecbase,
            SpecialReg::Exccause => self.sr.exccause,
            SpecialReg::Debugcause => self.sr.debugcause,
            SpecialReg::Ccount => self.timers.ccount(self.cycle_count),
            SpecialReg::Prid => self.prid,
            SpecialReg::Icount => self.sr.icount,
            SpecialReg::Icountlevel => self.sr.icountlevel,
            SpecialReg::Excvaddr => self.sr.excvaddr,
            SpecialReg::Ccompare0 => self.timers.ccompare[0],
            SpecialReg::Ccompare1 => self.timers.ccompare[1],
            SpecialReg::Ccompare2 => self.timers.ccompare[2],
            SpecialReg::Misc0 => self.sr.misc[0],
            SpecialReg::Misc1 => self.sr.misc[1],
            SpecialReg::Misc2 => self.sr.misc[2],
            SpecialReg::Misc3 => self.sr.misc[3],
        })
    }

    /// Write a special register with its side effects. Returns whether this
    /// is a poll-point-(b) write.
    pub(super) fn sr_write<T: Tracer + ?Sized>(
        &mut self,
        bus: &mut B,
        reg: SpecialReg,
        v: u32,
        tracer: &mut T,
    ) -> Result<bool, Trap> {
        let now = self.cycle_count;
        match reg {
            // The third invalidation event (`super::translated`): a translated
            // block may have inlined the LEND it saw, and `restore_context`
            // rewrites all three on every context switch. Cheap to raise, and
            // the failure it prevents is silent. The `LOOP` instruction writes
            // them too and is deliberately *not* an event — see that module.
            SpecialReg::Lbeg => {
                self.sr.lbeg = v;
                self.invalidate_blocks();
            }
            SpecialReg::Lend => {
                self.sr.lend = v;
                self.invalidate_blocks();
            }
            SpecialReg::Lcount => {
                self.sr.lcount = v;
                self.invalidate_blocks();
            }
            // SAR is 6 bits (Table 5-135).
            SpecialReg::Sar => self.cpu.sar = v & 0x3F,
            SpecialReg::Br => self.write_br(v, tracer),
            SpecialReg::Litbase => self.sr.litbase = v,
            SpecialReg::Scompare1 => self.sr.scompare1 = v,
            SpecialReg::Acclo => self.mac.set_acclo(v),
            SpecialReg::Acchi => self.mac.set_acchi(v),
            SpecialReg::M0 => self.mac.mr[0] = v,
            SpecialReg::M1 => self.mac.mr[1] = v,
            SpecialReg::M2 => self.mac.mr[2] = v,
            SpecialReg::M3 => self.mac.mr[3] = v,
            SpecialReg::WindowBase => self.cpu.window_base = (v & 0xF) as u8,
            SpecialReg::WindowStart => self.cpu.window_start = v as u16,
            // Trace-port id: no trace port here.
            SpecialReg::Mmid => {}
            SpecialReg::IbreakEnable => self.breaks.ibreakenable = v & 0b11,
            SpecialReg::Memctl => self.sr.memctl = v,
            SpecialReg::Atomctl => self.sr.atomctl = v,
            SpecialReg::Ddr => self.sr.ddr = v,
            SpecialReg::Ibreaka0 => self.breaks.ibreaka[0] = v,
            SpecialReg::Ibreaka1 => self.breaks.ibreaka[1] = v,
            SpecialReg::Dbreaka0 | SpecialReg::Dbreakc0 => {
                if reg == SpecialReg::Dbreaka0 {
                    self.breaks.dbreaka[0] = v;
                } else {
                    self.breaks.dbreakc[0] = v;
                }
                bus.set_watchpoint(0, self.breaks.watchpoint(0));
            }
            SpecialReg::Dbreaka1 | SpecialReg::Dbreakc1 => {
                if reg == SpecialReg::Dbreaka1 {
                    self.breaks.dbreaka[1] = v;
                } else {
                    self.breaks.dbreakc[1] = v;
                }
                bus.set_watchpoint(1, self.breaks.watchpoint(1));
            }
            SpecialReg::Epc1 => self.sr.epc[1] = v,
            SpecialReg::Epc2 => self.sr.epc[2] = v,
            SpecialReg::Epc3 => self.sr.epc[3] = v,
            SpecialReg::Epc4 => self.sr.epc[4] = v,
            SpecialReg::Epc5 => self.sr.epc[5] = v,
            SpecialReg::Epc6 => self.sr.epc[6] = v,
            SpecialReg::Epc7 => self.sr.epc[7] = v,
            SpecialReg::Depc => self.sr.depc = v,
            SpecialReg::Eps2 => self.sr.eps[2] = v,
            SpecialReg::Eps3 => self.sr.eps[3] = v,
            SpecialReg::Eps4 => self.sr.eps[4] = v,
            SpecialReg::Eps5 => self.sr.eps[5] = v,
            SpecialReg::Eps6 => self.sr.eps[6] = v,
            SpecialReg::Eps7 => self.sr.eps[7] = v,
            SpecialReg::Excsave1 => self.sr.excsave[1] = v,
            SpecialReg::Excsave2 => self.sr.excsave[2] = v,
            SpecialReg::Excsave3 => self.sr.excsave[3] = v,
            SpecialReg::Excsave4 => self.sr.excsave[4] = v,
            SpecialReg::Excsave5 => self.sr.excsave[5] = v,
            SpecialReg::Excsave6 => self.sr.excsave[6] = v,
            SpecialReg::Excsave7 => self.sr.excsave[7] = v,
            SpecialReg::Cpenable => self.cpu.cpenable = v & 0xFF,
            // SR 226 written is INTSET: raises software lines — (b).
            SpecialReg::Interrupt => {
                self.ints.intset(v);
                return Ok(true);
            }
            SpecialReg::Intclear => self.ints.intclear(v),
            SpecialReg::Intenable => {
                self.ints.intenable = v;
                return Ok(true);
            }
            SpecialReg::Ps => {
                self.set_ps_raw(v);
                return Ok(true);
            }
            SpecialReg::Vecbase => self.sr.vecbase = v,
            SpecialReg::Exccause => self.sr.exccause = v & super::sr::EXCCAUSE_MASK,
            // "WSR Function: Reserved" (Table 5-159).
            SpecialReg::Debugcause => {}
            SpecialReg::Ccount => {
                self.timers.write_ccount(now, v);
            }
            SpecialReg::Prid => return Err(illegal()),
            SpecialReg::Icount => self.sr.icount = v,
            SpecialReg::Icountlevel => self.sr.icountlevel = v & 0xF,
            SpecialReg::Excvaddr => self.sr.excvaddr = v,
            // A CCOMPARE write clears its timer's request (§4.4.6.2).
            SpecialReg::Ccompare0 | SpecialReg::Ccompare1 | SpecialReg::Ccompare2 => {
                let i = match reg {
                    SpecialReg::Ccompare0 => 0,
                    SpecialReg::Ccompare1 => 1,
                    _ => 2,
                };
                self.timers.write_ccompare(now, i, v);
                self.ints.timer_cleared(i);
            }
            SpecialReg::Misc0 => self.sr.misc[0] = v,
            SpecialReg::Misc1 => self.sr.misc[1] = v,
            SpecialReg::Misc2 => self.sr.misc[2] = v,
            SpecialReg::Misc3 => self.sr.misc[3] = v,
        }
        Ok(false)
    }

    /// The debug level gate every debug exception shares (RM §4.7.6.4).
    #[inline]
    #[must_use]
    pub fn debug_exceptions_enabled(&self) -> bool {
        InterruptUnit::cintlevel(self.ps()) < DEBUGLEVEL
    }
}
