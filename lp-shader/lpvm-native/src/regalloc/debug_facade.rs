//! Centralized debug/trace wiring for the allocator (`feature = "debug"`).
//!
//! Keeps `TraceSink`, `TracePush`, and trace-derived helpers in one place so
//! `walk.rs` and `AllocOutput` stay free of scattered `#[cfg(feature = "debug")]`.

use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

pub use crate::regalloc::trace::TraceEntry;

/// Trace sink: real [`crate::regalloc::trace::AllocTrace`] when `debug` is enabled, ZST otherwise.
#[cfg(feature = "debug")]
pub type TraceSink = crate::regalloc::trace::AllocTrace;

/// Trace sink: ZST when `debug` is disabled (no storage, no allocations).
#[cfg(not(feature = "debug"))]
pub type TraceSink = ();

/// Unified lazy `push` for [`TraceSink`]: records entries in debug builds,
/// no-op otherwise.
///
/// Takes a closure so entry construction (`String::from` + `format!`) only
/// happens when the `debug` feature is enabled. With the ZST sink the closure
/// is never invoked and the formatting compiles away — this matters on
/// device, where eagerly-built trace strings were the largest `format!`
/// source in the compile path.
pub trait TracePush {
    fn push_with(&mut self, f: impl FnOnce() -> TraceEntry);
}

#[cfg(feature = "debug")]
impl TracePush for TraceSink {
    fn push_with(&mut self, f: impl FnOnce() -> TraceEntry) {
        self.entries.push(f());
    }
}

#[cfg(not(feature = "debug"))]
impl TracePush for TraceSink {
    fn push_with(&mut self, _f: impl FnOnce() -> TraceEntry) {}
}

/// New empty trace sink for allocator state.
pub fn trace_sink_new() -> TraceSink {
    #[cfg(feature = "debug")]
    {
        crate::regalloc::trace::AllocTrace::new()
    }
    #[cfg(not(feature = "debug"))]
    {
        ()
    }
}

#[cfg(feature = "debug")]
fn is_entry_trace_mnemonic(m: &str) -> bool {
    m == "entry" || m == "entry_move" || m == "entry_spill" || m == "entry_slot_init"
}

/// Non-entry trace rows grouped by VInst index (forward order).
pub fn trace_by_vinst_or_empty(
    output: &crate::regalloc::AllocOutput,
) -> VecMap<usize, Vec<&TraceEntry>> {
    #[cfg(feature = "debug")]
    {
        let trace = &output.trace;
        let mut map: VecMap<usize, Vec<&TraceEntry>> = VecMap::new();
        for entry in trace.entries.iter().rev() {
            if !is_entry_trace_mnemonic(&entry.vinst_mnemonic) {
                map.entry(entry.vinst_idx).or_default().push(entry);
            }
        }
        for v in map.values_mut() {
            v.reverse();
        }
        map
    }
    #[cfg(not(feature = "debug"))]
    {
        let _ = output;
        VecMap::new()
    }
}

/// Append entry-trace metadata lines (ABI entry moves) after spill/ret lines in interleaved render.
pub fn append_entry_trace_metadata_lines(
    lines: &mut Vec<String>,
    ind_lp: &str,
    output: &crate::regalloc::AllocOutput,
) {
    #[cfg(feature = "debug")]
    {
        for entry in &output.trace.entries {
            if is_entry_trace_mnemonic(&entry.vinst_mnemonic) {
                lines.push(format!(
                    "{ind_lp}; {}: {}",
                    entry.vinst_mnemonic, entry.decision
                ));
            }
        }
    }
    #[cfg(not(feature = "debug"))]
    {
        let _ = (lines, ind_lp, output);
    }
}
