//! Fuel-check emission (mirrors the rv32 design in
//! `docs/adr/2026-07-20-lpvm-native-fuel.md`).
//!
//! Unit = loop back-edge executions. Placement: check-then-decrement
//! immediately before each loop's single back-edge `br`, check-only after the
//! shadow-stack prologue at function entry. The trap fires when a check
//! **observes** 0; reaching 0 on the final decrement and returning is not a
//! trap (this is what lets filetests assert exact `armed − N` remainders).
//!
//! Abort transport: on trap the check stores
//! [`lpvm::TRAP_CODE_OUT_OF_FUEL`] to the vmctx trap slot and executes
//! `unreachable` — wasm unwinds to the host in one shot (no rv32-style
//! epilogue cascade needed). The host classifies by reading the trap slot,
//! not the runtime's error message.

use wasm_encoder::{BlockType, InstructionSink};

use crate::emit::memory;

/// Function-entry fuel check: load the fuel low word, trap if it is already
/// 0 (no decrement — loop-free functions consume nothing, preserving
/// `__lp_get_fuel()` semantics pinned by `filetests/vmcontext/fuel-read.glsl`).
pub(crate) fn emit_entry_fuel_check(sink: &mut InstructionSink<'_>, vmctx: u32) {
    emit_check_and_trap(sink, vmctx);
}

/// Loop back-edge fuel check: trap if the fuel low word is 0, otherwise
/// decrement it by one. Emitted immediately before the loop's back-edge
/// `br`, so it executes exactly once per back-edge traversal.
pub(crate) fn emit_backedge_fuel_check(sink: &mut InstructionSink<'_>, vmctx: u32) {
    emit_check_and_trap(sink, vmctx);
    // fuel = fuel - 1
    sink.local_get(vmctx)
        .local_get(vmctx)
        .i32_load(memory::mem_arg0(lpvm::VMCTX_OFFSET_FUEL as u32, 2))
        .i32_const(1)
        .i32_sub()
        .i32_store(memory::mem_arg0(lpvm::VMCTX_OFFSET_FUEL as u32, 2));
}

/// `if (fuel == 0) { vmctx.trap = OUT_OF_FUEL; unreachable }` — the shared
/// prefix of both check shapes. The `if`/`end` pair is balanced inline and
/// contains no branches, so surrounding wasm branch depths are unaffected.
fn emit_check_and_trap(sink: &mut InstructionSink<'_>, vmctx: u32) {
    sink.local_get(vmctx)
        .i32_load(memory::mem_arg0(lpvm::VMCTX_OFFSET_FUEL as u32, 2))
        .i32_eqz()
        .if_(BlockType::Empty);
    sink.local_get(vmctx)
        .i32_const(lpvm::TRAP_CODE_OUT_OF_FUEL as i32)
        .i32_store(memory::mem_arg0(lpvm::VMCTX_OFFSET_TRAP as u32, 2))
        .unreachable();
    sink.end();
}
