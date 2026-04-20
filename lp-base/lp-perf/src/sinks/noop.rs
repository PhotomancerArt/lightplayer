use crate::JitSymbolEntry;
use crate::PerfEventKind;

#[inline(always)]
pub fn emit(_name: &'static str, _kind: PerfEventKind) {}

#[inline(always)]
pub fn emit_jit_map_load(_base: u32, _len: u32, _entries: &[JitSymbolEntry]) {}
