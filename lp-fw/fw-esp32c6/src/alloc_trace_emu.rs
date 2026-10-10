//! Diagnostic: every heap allocation and free, handed to the EMULATOR.
//!
//! Feature `alloc_trace_emu`; **never shipped, and never flashed**: each hook
//! is a bare `ebreak; ret`, so on silicon the first allocation traps. On
//! `lp-emu:esp32c6:*` the emulator claims both addresses
//! (`lp-cli emu run --alloc-trace <file> --alloc-trace-elf p2.elf`), reads
//! the arguments out of `a0..a3` and the frame-pointer chain out of guest
//! memory, writes one line per call, and returns to `ra` — so the firmware
//! keeps no table, holds no lock and allocates nothing for it, and the heap's
//! layout is the shipped image's up to the few bytes of `.rwtext` these two
//! functions and esp-alloc's hook calls take.
//!
//! Written for the RAM research program's E8 (attributing a project load's
//! heap), because `heap_track_diag`'s LP SRAM table holds ~300 live
//! allocations and a first frame makes thousands. The hooks are esp-alloc's
//! own `alloc-hooks` (`_esp_alloc_alloc` after every `alloc_caps`, with the
//! returned pointer; `_esp_alloc_dealloc` before every `dealloc`), so a
//! `realloc` is an alloc and a dealloc, as the allocator sees it.
//!
//! Both live in RAM (`.rwtext`): esp-alloc runs from interrupts and under a
//! disabled flash cache, so a hook in flash would be a hazard the shipped
//! image does not have.

#[cfg(feature = "heap_track_diag")]
compile_error!("`alloc_trace_emu` and `heap_track_diag` both define esp-alloc's hooks: pick one");

// The two symbols esp-alloc's `alloc-hooks` declares (`extern "Rust"`), in
// assembly: a naked `extern "Rust"` fn is unstable, and the hooks need no
// prologue at all — the emulator returns to `ra` from the first instruction.
//
// `_esp_alloc_alloc(heap: &EspHeap, caps: EnumSet<MemoryCapability>, ptr:
// usize, size: usize)`: `a0..a3`. `_esp_alloc_dealloc(heap: &EspHeap, ptr:
// usize, size: usize)`: `a0..a2`. `_lp_alloc_trace_mark(text: *const u8,
// len: usize)` is fw-esp32-common's `alloc-trace-marks`: every log record, at
// the instant it is logged, so a trace's points are exact rather than "when
// the host saw the line". Under the emulator the `ret` is never reached.
core::arch::global_asm!(
    ".pushsection .rwtext, \"ax\", @progbits",
    ".p2align 2",
    ".globl _esp_alloc_alloc",
    ".type _esp_alloc_alloc, @function",
    "_esp_alloc_alloc:",
    "ebreak",
    "ret",
    ".size _esp_alloc_alloc, . - _esp_alloc_alloc",
    ".p2align 2",
    ".globl _esp_alloc_dealloc",
    ".type _esp_alloc_dealloc, @function",
    "_esp_alloc_dealloc:",
    "ebreak",
    "ret",
    ".size _esp_alloc_dealloc, . - _esp_alloc_dealloc",
    ".p2align 2",
    ".globl _lp_alloc_trace_mark",
    ".type _lp_alloc_trace_mark, @function",
    "_lp_alloc_trace_mark:",
    "ebreak",
    "ret",
    ".size _lp_alloc_trace_mark, . - _lp_alloc_trace_mark",
    ".popsection",
);
