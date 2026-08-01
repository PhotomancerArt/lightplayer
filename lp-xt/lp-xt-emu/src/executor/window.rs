//! The window machinery: `ENTRY`, `RETW`, `RET`, and window overflow/underflow
//! implemented *directly* (spill/reload to the ABI stack save areas) rather than
//! by emulating the `_WindowOverflow`/`_WindowUnderflow` handler vectors.
//!
//! ## Model
//!
//! `WindowStart` bit `k` set ⇒ the frame based at window `k` is *resident* in the
//! physical register file. A frame entered with call-increment `inc` owns `inc`
//! base-units (`4*inc` physical registers) starting at its base — those are the
//! registers that must be preserved across its lifetime and that get
//! spilled/reloaded.
//!
//! - **ENTRY** rotates `WindowBase` forward by `PS.CALLINC`. Before committing,
//!   if the physical registers the new frame will own are still owned by a
//!   resident ancestor (the ring wrapped around), that ancestor is spilled — a
//!   window *overflow*.
//! - **RETW** rotates back by the call-increment recorded in `a0`'s top two
//!   bits. If the frame being returned into is not resident (it was spilled),
//!   its registers are reloaded — a window *underflow*.
//!
//! ## Save-area placement
//!
//! A spilled frame's registers go to a stack save area located from the frame's
//! own recorded SP: register group `g` (4 regs) at `[sp - 16*(g+1) .. sp -
//! 16*g)`. The base group (`a0..a3` at `[sp-16, sp)`) matches the Xtensa ABI;
//! the extra groups (`a4..a7`, `a8..a11`) are placed just below rather than in
//! the caller's frame as the hardware handlers do. For a conforming frame
//! (frame size == `4*inc` bytes, as the ABI requires and the corpus uses) the
//! save areas of adjacent frames tile without overlap, and — since bare
//! payloads never read another frame's save area — the observable result is
//! identical to hardware. This is the deliberate "model the effect, not the
//! handler vectors" choice (see the milestone spec and README).

use lp_xt_inst::Inst;

use crate::cpu::{Cpu, FrameRec, NUM_BASES};
use crate::emu::{Emulator, Flow};
use crate::error::Trap;
use crate::trace::{TraceEvent, Tracer};

/// Address of the save slot for a frame's register `r` (0-based within the
/// frame), located from the frame's *callee's* stack pointer `callee_sp`.
///
/// Group 0 (`a0..a3`) lands at `[callee_sp-16, callee_sp)` — the Xtensa ABI base
/// save area (which holds the returning frame's `a0`/`a1`, the registers a
/// window reload actually needs). Wider frames (`a4..`, `a8..`) tile just below;
/// the exact placement of those extra groups is not observable for bare payloads
/// (see the module docs / README).
#[inline]
fn save_slot(callee_sp: u32, r: u8) -> u32 {
    let group = (r / 4) as u32;
    let within = (r % 4) as u32;
    callee_sp
        .wrapping_sub(16 * (group + 1))
        .wrapping_add(within * 4)
}

impl Emulator {
    pub(super) fn exec_entry(
        &mut self,
        inst: &Inst,
        tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        let (as_reg, imm) = match *inst {
            Inst::Entry(rs, imm) => (rs.num(), imm),
            _ => unreachable!("exec_entry got {inst:?}"),
        };
        let inc = self.cpu.ps_callinc.max(1);
        let old_base = self.cpu.window_base;
        let new_base = (old_base + inc) % NUM_BASES;

        // Overflow: make room for the new frame's owned registers.
        self.ensure_window_free(new_base, inc, tracer)?;

        // SP is read in the caller's (current) window, then the callee's SP is
        // written after the rotation.
        let old_sp = self.cpu.a(as_reg);
        let new_sp = old_sp.wrapping_sub(imm);

        self.cpu.window_base = new_base;
        self.wreg(as_reg, new_sp, tracer);
        self.cpu.window_start |= 1 << new_base;
        self.cpu.call_stack.push(FrameRec {
            base: new_base,
            sp: new_sp,
            inc,
            resident: true,
        });

        tracer.event(TraceEvent::WindowRotate {
            what: "entry",
            old_base,
            new_base,
            window_start: self.cpu.window_start,
        });
        Ok(Flow::Next)
    }

    pub(super) fn exec_retw(&mut self, tracer: &mut dyn Tracer) -> Result<Flow, Trap> {
        let a0 = self.rreg(0);
        let n = ((a0 >> 30) & 3) as u8;
        // Return PC: high 2 (region) bits from the current PC, low 30 from a0.
        let ret_pc = (self.cpu.pc & 0xC000_0000) | (a0 & 0x3FFF_FFFF);
        let old_base = self.cpu.window_base;
        let new_base = (old_base + NUM_BASES - n) % NUM_BASES;

        // The returning (current) frame's stack pointer locates the caller's
        // base save area at `[callee_sp - 16, callee_sp)`.
        let callee_sp = self
            .cpu
            .call_stack
            .last()
            .map(|f| f.sp)
            .unwrap_or(self.cpu.a(1));

        // The returning frame's registers are abandoned; pop it.
        self.cpu.window_start &= !(1 << old_base);
        self.cpu.call_stack.pop();

        // Underflow: the caller is now the innermost frame. Reload it if it was
        // spilled while this (or a descendant) frame was running.
        let caller_spilled = self.cpu.call_stack.last().is_some_and(|f| !f.resident);
        if caller_spilled {
            self.reload_frame(callee_sp, tracer)?;
        }
        self.cpu.window_start |= 1 << new_base;

        self.cpu.window_base = new_base;
        tracer.event(TraceEvent::WindowRotate {
            what: "retw",
            old_base,
            new_base,
            window_start: self.cpu.window_start,
        });
        Ok(Flow::Jump(ret_pc))
    }

    /// `RET` / `RET.N`: non-windowed return to the address in `a0`.
    pub(super) fn exec_ret(&mut self) -> Flow {
        Flow::Jump(self.rreg(0))
    }

    /// Spill any resident ancestor whose registers collide with the new frame's
    /// window.
    ///
    /// The new frame's live window is always 16 registers = 4 base-units
    /// `[new_base, new_base+4)`. The low `4-inc` units are shared with the
    /// immediate caller by design (the caller's out-registers are the callee's
    /// in-registers). The high `inc` units — `[new_base + (4-inc), new_base+4)`
    /// — are the new frame's *out* registers, where its own `CALL` will write
    /// the return address and stage arguments; if the register ring has wrapped
    /// so a live ancestor still occupies them, that ancestor must be spilled
    /// *now*, before the frame runs, or the next `CALL` would clobber it. Loops
    /// because a wide frame can collide with more than one ancestor.
    fn ensure_window_free(
        &mut self,
        new_base: u8,
        inc: u8,
        tracer: &mut dyn Tracer,
    ) -> Result<(), Trap> {
        let region_start = new_base + (4 - inc);
        loop {
            // Find the oldest resident frame that owns a unit in the region.
            let mut victim = None;
            'find: for i in 0..self.cpu.call_stack.len() {
                let f = self.cpu.call_stack[i];
                if !f.resident {
                    continue;
                }
                for k in 0..f.inc {
                    let unit = (f.base + k) % NUM_BASES;
                    for u in 0..inc {
                        if (region_start + u) % NUM_BASES == unit {
                            victim = Some(i);
                            break 'find;
                        }
                    }
                }
            }
            match victim {
                None => return Ok(()),
                Some(i) => self.spill_frame(i, tracer)?,
            }
        }
    }

    /// Spill call-stack frame `i`'s owned registers to its stack save area
    /// (located from its callee's SP — the next resident frame) and mark it
    /// non-resident.
    fn spill_frame(&mut self, i: usize, tracer: &mut dyn Tracer) -> Result<(), Trap> {
        let f = self.cpu.call_stack[i];
        // The victim is the oldest resident frame, so the frame after it is
        // resident and its SP locates the victim's base save area.
        let callee_sp = self.cpu.call_stack[i + 1].sp;
        let nregs = 4 * f.inc;
        for r in 0..nregs {
            let phys = Cpu::phys_at(f.base, r);
            let val = self.cpu.ar[phys];
            self.mem.write_u32(save_slot(callee_sp, r), val)?;
        }
        self.cpu.call_stack[i].resident = false;
        self.cpu.window_start &= !(1 << f.base);
        tracer.event(TraceEvent::WindowSpill {
            base: f.base,
            sp: callee_sp,
            nregs,
        });
        Ok(())
    }

    /// Reload the innermost (caller) frame's owned registers from its stack save
    /// area (located from its callee's SP, `callee_sp`) and mark it resident.
    fn reload_frame(&mut self, callee_sp: u32, tracer: &mut dyn Tracer) -> Result<(), Trap> {
        let idx = self.cpu.call_stack.len() - 1;
        let f = self.cpu.call_stack[idx];
        let nregs = 4 * f.inc;
        for r in 0..nregs {
            let val = self.mem.read_u32(save_slot(callee_sp, r))?;
            let phys = Cpu::phys_at(f.base, r);
            self.cpu.ar[phys] = val;
        }
        self.cpu.call_stack[idx].resident = true;
        self.cpu.window_start |= 1 << f.base;
        tracer.event(TraceEvent::WindowReload {
            base: f.base,
            sp: callee_sp,
            nregs,
        });
        Ok(())
    }
}
