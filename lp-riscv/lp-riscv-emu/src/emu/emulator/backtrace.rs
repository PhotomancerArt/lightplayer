//! Guest stack unwinding for backtrace generation.
//!
//! Walks the RISC-V call stack using the frame pointer (s0/fp) chain.
//! Requires firmware built with -C force-frame-pointers=yes.

extern crate alloc;

use super::super::memory::Memory;
use super::state::Riscv32Emulator;
use alloc::vec::Vec;

/// Maximum number of frames to unwind to avoid runaway on corrupted stacks.
const MAX_FRAMES: usize = 32;

/// RISC-V RAM start (stack lives in RAM).
const RAM_START: u32 = 0x8000_0000;

impl Riscv32Emulator {
    /// Unwind the guest call stack and return a list of addresses.
    ///
    /// Returns addresses in order: faulting PC first, then caller return addresses.
    /// Uses the RISC-V frame pointer (s0) chain. Stops on memory read errors or
    /// invalid fp, returning the partial backtrace collected so far.
    ///
    /// # Arguments
    /// * `pc` - Program counter where the fault/error occurred
    /// * `regs` - Register state at the time of the fault
    ///
    /// # Returns
    /// Vec of addresses (faulting PC, then ra, then unwound return addresses)
    pub fn unwind_backtrace(&self, pc: u32, regs: &[i32; 32]) -> Vec<u32> {
        self.unwind_backtrace_inner(pc, regs, false)
    }

    /// Like unwind_backtrace but prints diagnostic info about the fp chain.
    #[cfg(feature = "std")]
    pub fn unwind_backtrace_debug(&self, pc: u32, regs: &[i32; 32]) -> Vec<u32> {
        self.unwind_backtrace_inner(pc, regs, true)
    }

    fn unwind_backtrace_inner(&self, pc: u32, regs: &[i32; 32], _debug: bool) -> Vec<u32> {
        #[cfg(feature = "std")]
        macro_rules! dbg_print {
            ($debug:expr, $($arg:tt)*) => {
                if $debug { std::eprintln!($($arg)*); }
            };
        }
        #[cfg(not(feature = "std"))]
        macro_rules! dbg_print {
            ($debug:expr, $($arg:tt)*) => {};
        }

        let mut addrs = Vec::with_capacity(MAX_FRAMES);
        let mem = self.memory();
        let ram_end = mem.ram_end();

        addrs.push(pc);
        dbg_print!(_debug, "  bt[0] pc=0x{pc:08x}");
        dbg_print!(_debug, "  ra=0x{:08x} s0/fp=0x{:08x} ram_end=0x{ram_end:08x}",
            regs[1] as u32, regs[8] as u32);

        let ra = regs[1] as u32;
        if is_valid_code_address(ra, mem) {
            addrs.push(ra);
            dbg_print!(_debug, "  bt[1] ra=0x{ra:08x} (valid code)");
        } else {
            dbg_print!(_debug, "  bt[1] ra=0x{ra:08x} (INVALID code addr)");
        }

        // fp must be in RAM and aligned. Allow fp == ram_end because the
        // entry code sets s0 = __stack_start which equals ram_end; reads at
        // [fp-4] and [fp-8] are still in bounds.
        let mut fp = regs[8] as u32;
        if fp >= RAM_START && fp <= ram_end && fp % 4 == 0 {
            match mem.read_word(fp.wrapping_sub(8)) {
                Ok(pfp) => {
                    dbg_print!(_debug, "  advance: [0x{:08x}]=0x{:08x}",
                        fp.wrapping_sub(8), pfp as u32);
                    if (pfp as u32) >= RAM_START {
                        fp = pfp as u32;
                    } else {
                        dbg_print!(_debug, "  STOP: prev_fp 0x{:08x} < RAM_START", pfp as u32);
                        return addrs;
                    }
                }
                _ => {
                    dbg_print!(_debug, "  STOP: read_word failed at 0x{:08x}", fp.wrapping_sub(8));
                    return addrs;
                }
            }
        } else {
            dbg_print!(_debug, "  STOP: initial fp=0x{fp:08x} invalid (ram_end=0x{ram_end:08x})");
            return addrs;
        }

        let mut frame_count = addrs.len();
        while frame_count < MAX_FRAMES {
            if fp < RAM_START || fp > ram_end || fp % 4 != 0 {
                dbg_print!(_debug, "  STOP: fp=0x{fp:08x} out of range or unaligned");
                break;
            }

            let saved_ra = match mem.read_word(fp.wrapping_sub(4)) {
                Ok(v) => v as u32,
                Err(_) => {
                    dbg_print!(_debug, "  STOP: read_word failed for ra at 0x{:08x}", fp.wrapping_sub(4));
                    break;
                }
            };
            let prev_fp = match mem.read_word(fp.wrapping_sub(8)) {
                Ok(v) => v,
                Err(_) => {
                    dbg_print!(_debug, "  STOP: read_word failed for fp at 0x{:08x}", fp.wrapping_sub(8));
                    break;
                }
            };

            dbg_print!(_debug, "  bt[{}] fp=0x{fp:08x} [fp-4]=0x{saved_ra:08x} [fp-8]=0x{:08x}{}",
                addrs.len(), prev_fp as u32,
                if is_valid_code_address(saved_ra, mem) { "" } else { " (ra INVALID)" });

            if is_valid_code_address(saved_ra, mem) {
                addrs.push(saved_ra);
            }

            let prev_fp_u32 = prev_fp as u32;
            // Stop if prev_fp is outside RAM or not advancing upward
            // (cycle detection — stack grows downward so caller frames have
            // higher addresses). Use unsigned comparison; RAM addresses
            // (0x80000000+) are negative as i32.
            if prev_fp_u32 < RAM_START || prev_fp_u32 <= fp {
                dbg_print!(_debug, "  STOP: prev_fp=0x{prev_fp_u32:08x} (fp=0x{fp:08x})");
                break;
            }
            fp = prev_fp_u32;
            frame_count += 1;
        }

        addrs
    }
}

/// Heuristic: address looks like a valid code address (in ROM range).
fn is_valid_code_address(addr: u32, mem: &Memory) -> bool {
    if addr == 0 {
        return false;
    }
    // Code is in low memory; RAM starts at 0x80000000
    if addr >= RAM_START {
        return false;
    }
    // Check it's within code bounds
    let code_start = mem.code_start();
    let offset = addr.wrapping_sub(code_start) as usize;
    offset < mem.code().len()
}
