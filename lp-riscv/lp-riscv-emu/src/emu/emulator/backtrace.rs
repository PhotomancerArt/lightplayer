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
        let mut addrs = Vec::with_capacity(MAX_FRAMES);
        let mem = self.memory();
        let ram_end = mem.ram_end();

        // Frame 0: faulting PC
        addrs.push(pc);

        // Frame 1: ra (return address of caller - we are in the callee)
        let ra = regs[1] as u32;
        if is_valid_code_address(ra, mem) {
            addrs.push(ra);
        }

        // Walk fp chain: s0 (x8) is frame pointer
        let mut fp = regs[8] as u32;
        let mut frame_count = addrs.len();

        while frame_count < MAX_FRAMES {
            // Validate fp is in RAM (stack)
            if fp < RAM_START || fp >= ram_end {
                break;
            }
            // Require fp to be 4-byte aligned for reading two words
            if fp % 4 != 0 {
                break;
            }

            // RISC-V psabi: saved ra at [fp], previous fp at [fp-4]
            let saved_ra = match mem.read_word(fp) {
                Ok(v) => v as u32,
                Err(_) => break,
            };
            let prev_fp = match mem.read_word(fp.wrapping_sub(4)) {
                Ok(v) => v,
                Err(_) => break,
            };

            if is_valid_code_address(saved_ra, mem) {
                addrs.push(saved_ra);
            }

            if prev_fp <= 0 || (prev_fp as u32) < RAM_START {
                break;
            }
            fp = prev_fp as u32;
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
