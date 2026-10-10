//! The allocation trace: every heap allocation and free the guest makes, with
//! its frame-pointer backtrace, written by the host as it happens.
//!
//! A firmware built for it (fw-esp32c6's `alloc_trace_emu`) has its
//! allocator call two functions whose first instruction is `ebreak`, one
//! after each allocation (`a2` = the pointer, `a3` = the size, `a1` = the
//! capability set) and one before each free (`a1` = the pointer, `a2` = the
//! size). [`install`] claims both addresses in the hook table — without
//! patching anything, the `ebreak` is the firmware's own — and each hook
//! writes one line and returns to `ra`. So the guest keeps no table and
//! allocates nothing for its trace, and the trace is complete: no slot runs
//! out, no event is sampled.
//!
//! The format, one event per line, every number in the guest's terms:
//!
//! ```text
//! # lp-alloc-trace 1
//! A <cycle> <ptr hex> <size> <caps> <ret hex>,<ret hex>,…   an allocation (ptr 0: it failed)
//! F <cycle> <ptr hex> <size>                                a free
//! M <cycle> <text>                                          a host marker (a console line)
//! L <cycle> <text>                                          a guest log record, as logged
//! ```
//!
//! `L` lines come from a third hook, `_lp_alloc_trace_mark(text, len)`, which
//! the firmware calls with every log record it formats (fw-esp32-common's
//! `alloc-trace-marks`): their cycle is the instant of the log call, so a
//! point taken at one is exact. `M` lines (and the `@reboot` marker the
//! machine writes when the chip resets) are what the host saw.
//!
//! `<ret>` are RETURN addresses, innermost first: the hook's own `ra` (inside
//! the allocator), then the saved `ra` of each frame on the `s0` chain.
//! Symbolize at `ret - 1` (inside the call instruction). The walk stops at
//! [`MAX_FRAMES`], at a frame pointer outside DRAM, or at one that does not
//! grow (the chain's end), so the firmware must be built with
//! `-C force-frame-pointers` (fw-esp32c6 always is).
//!
//! Markers are what the host knew when it knew it: a console line reaches
//! the host up to a slice after the guest wrote it, so a marker's cycle is a
//! bound, not the instant of the log call.

use std::io::Write;

use crate::machine::Esp32C6Machine;
use crate::rom::HookResult;

/// The deepest backtrace a line carries.
pub const MAX_FRAMES: usize = 32;

/// The C6's HP SRAM, where every stack and so every frame pointer lives.
const DRAM: core::ops::Range<u32> = 0x4080_0000..0x4088_0000;

/// The trace's sink and its counters.
pub struct AllocTrace {
    out: Box<dyn Write + Send>,
    /// Allocations written (failed ones included).
    pub allocs: u64,
    /// Allocations that returned null.
    pub failed: u64,
    /// Frees written.
    pub frees: u64,
    /// Markers written.
    pub markers: u64,
    /// The first write error, if any; the trace stops writing after it.
    pub error: Option<String>,
}

impl AllocTrace {
    /// A trace into `out`, its header already written.
    pub fn new(mut out: Box<dyn Write + Send>, header: &[String]) -> Self {
        let mut error = writeln!(out, "# lp-alloc-trace 1")
            .err()
            .map(|e| e.to_string());
        for line in header {
            if error.is_none() {
                error = writeln!(out, "# {line}").err().map(|e| e.to_string());
            }
        }
        Self {
            out,
            allocs: 0,
            failed: 0,
            frees: 0,
            markers: 0,
            error,
        }
    }

    /// Write a host marker at `cycle`: one line, newlines flattened.
    pub fn marker(&mut self, cycle: u64, text: &str) {
        let text = text.replace(['\n', '\r'], " ");
        self.markers += 1;
        self.write(format_args!("M {cycle} {text}"));
    }

    /// Flush the sink; the run's end.
    pub fn flush(&mut self) {
        if let Err(e) = self.out.flush()
            && self.error.is_none()
        {
            self.error = Some(e.to_string());
        }
    }

    /// One line describing what the trace holds, for a run's report.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "alloc trace: {} allocation(s) ({} failed), {} free(s), {} marker(s)",
            self.allocs, self.failed, self.frees, self.markers
        );
        if let Some(e) = &self.error {
            s.push_str(&format!(" — WRITE ERROR, trace incomplete: {e}"));
        }
        s
    }

    fn write(&mut self, line: core::fmt::Arguments<'_>) {
        if self.error.is_some() {
            return;
        }
        if let Err(e) = writeln!(self.out, "{line}") {
            self.error = Some(e.to_string());
        }
    }
}

/// Claim the two hook addresses and start writing `trace`.
///
/// `alloc_at` / `dealloc_at` are the addresses of the firmware's
/// `_esp_alloc_alloc` / `_esp_alloc_dealloc`, read from its ELF (for a split
/// image, `p2.elf`: the loader's ELF does not name them). Nothing is patched
/// and nothing is checked here — a split image's core is not in RAM until its
/// loader has copied it — because the firmware's own `ebreak` there is what
/// runs a hook at all: an image without it never reaches one.
pub fn install(machine: &mut Esp32C6Machine, alloc_at: u32, dealloc_at: u32, trace: AllocTrace) {
    machine.set_alloc_trace(trace);
    machine
        .hooks_mut()
        .claim(alloc_at, "_esp_alloc_alloc (alloc trace)", on_alloc);
    machine
        .hooks_mut()
        .claim(dealloc_at, "_esp_alloc_dealloc (alloc trace)", on_dealloc);
}

/// Claim the log-record hook (`_lp_alloc_trace_mark`) as well: an `L` line
/// per record, at the instant it is logged.
pub fn install_marks(machine: &mut Esp32C6Machine, mark_at: u32) {
    machine
        .hooks_mut()
        .claim(mark_at, "_lp_alloc_trace_mark (alloc trace)", on_mark);
}

/// The longest record text read (the firmware cuts records shorter).
const MAX_MARK: u32 = 512;

fn on_mark(m: &mut Esp32C6Machine) -> HookResult {
    let r = m.registers();
    let (ptr, len) = (r[10], r[11].min(MAX_MARK));
    let mut bytes = Vec::with_capacity(len as usize);
    let mut word_at = u32::MAX;
    let mut word = 0u32;
    for i in 0..len {
        let a = ptr.wrapping_add(i);
        if a & !3 != word_at {
            word_at = a & !3;
            word = m.peek_word(word_at).unwrap_or(0);
        }
        bytes.push((word >> (8 * (a & 3))) as u8);
    }
    let text = String::from_utf8_lossy(&bytes).replace(['\n', '\r'], " ");
    let cycle = m.cycles();
    if let Some(t) = m.alloc_trace_mut() {
        t.markers += 1;
        t.write(format_args!("L {cycle} {text}"));
    }
    HookResult::Ret
}

fn on_alloc(m: &mut Esp32C6Machine) -> HookResult {
    let r = m.registers();
    let (caps, ptr, size) = (r[11], r[12], r[13]);
    let frames = backtrace(m, r[1], r[8]);
    let cycle = m.cycles();
    if let Some(t) = m.alloc_trace_mut() {
        t.allocs += 1;
        if ptr == 0 {
            t.failed += 1;
        }
        t.write(format_args!("A {cycle} {ptr:x} {size} {caps} {frames}"));
    }
    HookResult::Ret
}

fn on_dealloc(m: &mut Esp32C6Machine) -> HookResult {
    let r = m.registers();
    let (ptr, size) = (r[11], r[12]);
    let cycle = m.cycles();
    if let Some(t) = m.alloc_trace_mut() {
        t.frees += 1;
        t.write(format_args!("F {cycle} {ptr:x} {size}"));
    }
    HookResult::Ret
}

/// `ra`, then the saved `ra` of each frame on the `s0` chain, comma-joined
/// in hex.
fn backtrace(m: &mut Esp32C6Machine, ra: u32, s0: u32) -> String {
    let mut out = format!("{ra:x}");
    let mut fp = s0;
    let mut n = 1;
    while n < MAX_FRAMES && DRAM.contains(&fp) && fp % 4 == 0 {
        let Some(saved_ra) = m.peek_word(fp.wrapping_sub(4)) else {
            break;
        };
        let Some(prev_fp) = m.peek_word(fp.wrapping_sub(8)) else {
            break;
        };
        if saved_ra != 0 {
            out.push_str(&format!(",{saved_ra:x}"));
            n += 1;
        }
        if prev_fp <= fp {
            break;
        }
        fp = prev_fp;
    }
    out
}
