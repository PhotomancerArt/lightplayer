//! Shadow stack layout and memory-related emission helpers.

use alloc::vec::Vec;

use lpir::IrFunction;
use wasm_encoder::{InstructionSink, MemArg};

const FRAME_ALIGN: u32 = 16;

pub(crate) fn align_up(n: u32, align: u32) -> u32 {
    (n + align - 1) / align * align
}

pub(crate) fn slot_offsets(func: &IrFunction) -> Vec<u32> {
    let mut offsets = Vec::new();
    let mut cur = 0u32;
    for slot in &func.slots {
        offsets.push(cur);
        cur = cur.saturating_add(slot.size);
    }
    offsets
}

pub(crate) fn aligned_frame_size(func: &IrFunction) -> u32 {
    let total: u32 = func.slots.iter().map(|s| s.size).sum();
    align_up(total, FRAME_ALIGN)
}

pub(crate) fn emit_shadow_prologue(sink: &mut InstructionSink<'_>, sp: u32, frame_size: u32) {
    if frame_size == 0 {
        return;
    }
    sink.global_get(sp)
        .i32_const(i32::try_from(frame_size).unwrap_or(i32::MAX))
        .i32_sub()
        .global_set(sp);
}

pub(crate) fn emit_shadow_epilogue(sink: &mut InstructionSink<'_>, sp: u32, frame_size: u32) {
    if frame_size == 0 {
        return;
    }
    sink.global_get(sp)
        .i32_const(i32::try_from(frame_size).unwrap_or(i32::MAX))
        .i32_add()
        .global_set(sp);
}

pub(crate) fn mem_arg0(offset: u32, align_pow2: u32) -> MemArg {
    MemArg {
        offset: u64::from(offset),
        align: align_pow2,
        memory_index: 0,
    }
}
