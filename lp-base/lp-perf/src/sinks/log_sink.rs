use crate::JitSymbolEntry;
use crate::PerfEventKind;

#[inline(always)]
pub fn emit(name: &'static str, kind: PerfEventKind) {
    log::trace!("perf {} {:?}", name, kind);
}

#[inline(always)]
pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry]) {
    log::debug!(
        "lp-perf: jit_map_load base=0x{base:08x} len={len} count={}",
        entries.len()
    );
}
