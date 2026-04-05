# OOM Recovery: Stack Unwinding via `unwinding` Crate

**Date:** 2026-03-12
**Context:** Same as transactional-alloc.md. This is an alternative approach that uses real stack unwinding instead of `setjmp`/`longjmp` + allocation tracking.

---

## Goals

Same as the transactional allocator approach:

1. **Isolate OOMs to the operation that caused them.**
2. **Reclaim all memory** allocated during the failed operation.
3. **Testable in the emulator.**

Different trade-off on goal 3 from the transactional approach:

- **Zero RAM overhead.** No per-allocation headers, no tracking lists. Destructors run normally via stack unwinding, so the heap is cleaned up the same way it would be on a normal return path.
- **Flash/ROM cost.** Requires `.eh_frame` sections and landing pads in the binary. This costs flash, not RAM.

---

## How It Works

The [`unwinding`](https://crates.io/crates/unwinding) crate (nbdd0121/unwinding) is a pure Rust implementation of the Itanium C++ Exception Handling ABI. It provides `catch_unwind` and `begin_panic` for `#![no_std]` bare-metal targets, including RV32.

When a panic occurs (e.g., from an OOM allocation error), the unwinder:

1. Reads `.eh_frame` sections from the binary to find unwind information for each stack frame
2. Walks the stack from the panic point back to the `catch_unwind` call
3. For each frame, calls destructors (`Drop` impls) for any live objects
4. Returns control to the `catch_unwind` call site with the panic payload

This is identical to how `std::panic::catch_unwind` works on hosted targets, but implemented without `libc` or OS support.

---

## What Changes

### Cargo dependencies

```toml
# In lp-alloc or fw-core (wherever the recovery boundary lives)
unwinding = { version = "0.2", default-features = false, features = [
    "unwinder",       # the core unwinder
    "fde-static",     # reads .eh_frame from linker-provided symbols (no libc)
    "personality",    # provides #[lang = eh_personality]
    "panic",          # provides begin_panic + catch_unwind (no libc)
] }
```

### Panic strategy

Switch from `panic = "abort"` to `panic = "unwind"` in the relevant Cargo profiles:

```toml
# Workspace Cargo.toml
[profile.release]
panic = "unwind"

# Or scoped to specific packages if possible
[profile.release.package.fw-esp32]
panic = "unwind"

[profile.release.package.fw-emu]
panic = "unwind"
```

Note: `panic = "unwind"` must be consistent across all crates in the dependency graph. You cannot mix `panic = "abort"` and `panic = "unwind"` in crates that are linked together. This means all of lp-engine, lp-server, lps-compiler, cranelift, etc. would be compiled with `panic = "unwind"`.

### Linker scripts

**fw-emu (`lp-riscv-emu-guest/memory.ld`)** — currently discards `.eh_frame`:

```ld
/DISCARD/ : {
    *(.eh_frame .eh_frame.*)
}
```

Replace with:

```ld
. = ALIGN(8);
PROVIDE(__executable_start = ORIGIN(ROM));
PROVIDE(__etext = .);  /* after .rodata, before .data */
PROVIDE(__eh_frame = .);
.eh_frame : { KEEP (*(.eh_frame)) *(.eh_frame.*) } > ROM
```

The `__executable_start`, `__etext`, and `__eh_frame` symbols are used by the `fde-static` feature to locate the unwind tables at runtime.

**fw-esp32** — uses esp-hal's linker scripts. ESP32's linker scripts already retain `.eh_frame` in flash (not discarded like the emulator). May need to add the `__eh_frame` symbol if not already provided. The esp-hal linker scripts would need to be checked/patched.

**lps-builtins-emu-app (`memory.ld`)** — same change as fw-emu.

### Panic handler

Replace `esp-backtrace`'s panic handler on ESP32 and the custom panic handler on fw-emu with the `unwinding` crate's personality and panic support:

```rust
// Remove:
use esp_backtrace as _; // panic handler

// The `unwinding` crate with `personality` + `panic` features provides
// #[lang = eh_personality] and the panic runtime automatically.
extern crate unwinding;
```

On fw-emu, the existing custom `#[panic_handler]` in `lp-riscv-emu-guest/src/panic.rs` would need to be adapted. With `panic = "unwind"`, the panic handler is only called for double-panics (panic during unwind). The normal panic path goes through the personality function and unwinder.

### Alloc error handler

```rust
#[alloc_error_handler]
fn on_alloc_error(layout: Layout) -> ! {
    // With panic = "unwind", this panic will be caught by catch_unwind
    panic!("OOM: failed to allocate {} bytes (align {})", layout.size(), layout.align());
}
```

No `setjmp`/`longjmp` needed. The panic unwinds normally.

---

## Usage

### Protected call site

```rust
use unwinding::panic::catch_unwind;

// In ShaderRuntime::recompile()
self.executable = None;

match catch_unwind(|| compile_shader(source, config, builtins)) {
    Ok(executable) => {
        self.executable = Some(executable);
        self.status = NodeStatus::Ok;
    }
    Err(_panic_payload) => {
        self.status = NodeStatus::Error("OOM during shader compilation".into());
    }
}
```

Key differences from the transactional allocator approach:

- **No `unsafe`.** `catch_unwind` is safe. No `setjmp`/`longjmp`, no raw pointer manipulation, no assembly.
- **Closures work.** No need for `fn` pointer restriction. The closure can capture shared references freely. (Mutable captures are still unwise — a panic during a partial mutation leaves inconsistent state — but that's a normal Rust concern, not specific to this approach.)
- **Destructors run.** Every `Vec`, `HashMap`, `String`, etc. on the stack between `catch_unwind` and the panic point gets its `Drop` called. Memory is freed through the normal path. No tracking, no rollback, no headers.
- **No allocator wrapper needed.** The global allocator stays exactly as it is today. No overhead on any allocation path, ever.

### What about `UnwindSafe`?

`std::panic::catch_unwind` requires `F: UnwindSafe`. The `unwinding` crate's `catch_unwind` may or may not enforce this. If it does, you may need `AssertUnwindSafe` for closures that capture mutable references. Since our closure for compilation is designed to be side-effect-free (captures only shared refs and owned data), this shouldn't be an issue.

---

## Costs

### Flash/ROM size increase

The main cost. Two sources:

1. **`.eh_frame` sections.** The compiler emits DWARF unwind information for every function. This is a table describing how to restore registers at each point in the function. Size is roughly proportional to code size. Typical overhead: **10-20% of text section size.**

2. **Landing pads.** With `panic = "unwind"`, the compiler generates cleanup code at each point where a destructor might need to run during unwinding. With `panic = "abort"`, these are omitted. Typical overhead: **5-15% of text section size.**

Combined estimate: **15-35% flash increase.** This is the primary concern.

However:
- Flash is less constrained than RAM. ESP32-C6 has 4-8 MB of flash vs. 320 KB of usable heap RAM.
- The fw-emu emulator puts code in ROM which has unlimited configured length (`0x80000000`).
- The real question is whether the ESP32 flash partition has enough room for the firmware + web UI + this overhead.

**This must be measured, not estimated.** Build the firmware with `panic = "unwind"` and `.eh_frame` retained, compare binary sizes. The actual overhead depends heavily on the code structure and optimization level (`opt-level = "z"` may interact differently with unwind info).

### RAM overhead

**Zero.** No per-allocation headers. No tracking data structures. No static buffers. The unwinder itself uses a small amount of stack during unwinding (walking frames, calling destructors), but this is temporary and bounded.

Compare to transactional allocator: with ~985 live allocations at peak during compilation (from alloc-trace data), the transactional approach adds **~23.6 KB** of RAM overhead (985 × 24 bytes per header). That's 7.4% of the 320 KB heap, consumed precisely when memory pressure is highest.

### Code size (in RAM, for fw-emu)

The fw-emu guest loads code into ROM, not RAM. Code size increase doesn't affect the heap.

### Runtime overhead on the normal path

Minimal. With `panic = "unwind"`, the compiler generates slightly different code (landing pads are present but not executed on the normal path). The CPU doesn't execute any extra instructions unless a panic actually occurs. Branch prediction handles the landing pad branches efficiently.

### Runtime overhead on the unwind path

The unwinder reads `.eh_frame` tables and walks the stack. This is slower than the transactional allocator's linked-list walk, but it only happens during OOM recovery — a rare event where latency doesn't matter.

---

## Risks and Concerns

### 1. Nightly features required

The `unwinding` crate requires nightly Rust for `#[lang = eh_personality]`. The project already uses nightly (for `alloc_error_handler`, `no_std` features, etc.), so this isn't a new constraint.

### 2. Crate maturity

The `unwinding` crate has ~134 stars, is maintained by one person (nbdd0121), and is the only pure-Rust `no_std` unwinder. It's used by a few bare-metal projects but isn't widely battle-tested on RV32 specifically. A bug in the unwinder during OOM recovery could be worse than the OOM itself.

Mitigation: test thoroughly in the emulator first. The emulator runs the same RV32 code and provides full visibility via alloc-trace and instruction logging.

### 3. JIT code boundary

Cranelift JIT-compiled code (shader execution) has no `.eh_frame` entries — the JIT doesn't emit unwind information. If a panic occurs during JIT execution, the unwinder cannot walk through those frames.

This is likely not a problem for OOM recovery because:
- OOMs occur during **compilation** (Cranelift codegen), not during **execution** (JIT-compiled shader running).
- Compilation is pure Rust code with full `.eh_frame` coverage.
- If an OOM somehow occurred during JIT execution (unlikely — JIT code doesn't allocate via the Rust allocator), it would be an unrecoverable panic.

### 4. `panic = "unwind"` is all-or-nothing

All crates in the dependency graph must use the same panic strategy. Switching to `panic = "unwind"` affects everything: Cranelift, regalloc2, lps-compiler, lp-engine, lp-server, etc. This increases binary size across the board, not just for the code paths we want to protect.

There is no way to say "use unwind for lp-engine but abort for cranelift." It's a whole-binary decision.

### 5. Interaction with `esp-backtrace`

`esp-backtrace` provides a panic handler designed for `panic = "abort"`. Switching to `panic = "unwind"` means replacing it with the `unwinding` crate's panic runtime. We'd lose `esp-backtrace`'s nice formatted output and stack traces on fatal panics (ones that aren't caught by `catch_unwind`).

Mitigation: implement a custom double-panic handler that logs similarly to `esp-backtrace` before aborting. Or use `esp-backtrace` only for its backtrace-printing functionality (not the panic handler feature).

### 6. Flash budget

The web UI is planned for flash. If the firmware binary grows 15-35%, that's flash space the web UI can't use. On a 4 MB flash with, say, a 1.5 MB firmware partition, a 30% increase is ~450 KB less for the web UI.

This is the key decision factor and must be measured on the actual binary.

---

## Comparison with Transactional Allocator

| | Transactional Allocator | Stack Unwinding |
|---|---|---|
| **RAM overhead during compilation** | ~24 KB (985 allocs × 24B headers) | Zero |
| **RAM overhead outside compilation** | Zero | Zero |
| **Flash/ROM cost** | Zero | ~15-35% (must measure) |
| **Safety** | `unsafe` (setjmp/longjmp, raw pointers) | Safe (`catch_unwind` is safe Rust) |
| **Complexity** | Custom allocator wrapper, assembly for setjmp/longjmp, tracking list | Crate dependency + linker script changes |
| **Leak risk** | Heap memory caught by tracking; non-heap resources could leak | Destructors run; nothing leaks |
| **Allocator changes** | Wraps global allocator with overhead on every alloc/dealloc during transaction | No allocator changes |
| **Dependencies** | None (custom code) | `unwinding` crate (nightly, single maintainer) |
| **Panic strategy** | `panic = "abort"` (unchanged) | `panic = "unwind"` (whole binary) |
| **Debugging** | Custom: alloc-trace integration, magic sentinels | Standard: panic payloads, backtraces |

**The core trade-off: 24 KB of RAM when you can least afford it, vs. flash space you may need for a web UI.**

---

## Recommendation

Measure the flash cost before deciding. The steps:

1. Build fw-emu with `panic = "unwind"`, retain `.eh_frame` in the linker script, add the `unwinding` crate. Compare ROM section sizes.
2. If the ROM increase is acceptable, try the same on fw-esp32. Compare flash usage against the partition budget with web UI accounted for.
3. Run a compilation test in the emulator with `catch_unwind`. Trigger an OOM (shrink the heap). Verify destructors run and heap is clean after recovery.

If flash cost is acceptable → use this approach. It's simpler, safer, and has zero RAM overhead.
If flash cost is too high → fall back to the transactional allocator approach, accepting the ~24 KB RAM cost during compilation.

A hybrid is also possible: use unwinding on fw-emu (ROM is unlimited) for development and testing, use the transactional allocator on fw-esp32 (flash-constrained) for production. The `oom_protected` API can be the same — only the implementation differs behind a feature flag.
