# ESP32 Stack Unwinding: Implementation Report

**Date:** 2026-03-13
**Status:** Complete — OOM recovery confirmed working on device (2026-03-13)
**Predecessor:** [OOM Recovery: Stack Unwinding via `unwinding` Crate](2026-03-12-oom-recovery/stack-unwinding.md)

---

## Summary

Implementing `catch_unwind`-based OOM recovery on ESP32-C6 (RISC-V, `no_std`, bare-metal)
required solving six layered problems, each hidden behind the previous one. This document
records what broke, why, and the final working configuration.

### Confirmed on device

```
[test_oom] Test 1: catching simple panic...
====================== PANIC ======================
panicked at lp-fw/fw-esp32/src/main.rs:181:17: test panic
[test_oom] Test 1 OK: simple panic caught

[test_oom] Test 2: catching OOM...
====================== PANIC ======================
panicked at lp-fw/fw-esp32/src/main.rs:45:5: memory allocation of 65536 bytes failed
[test_oom] Test 2 OK: OOM caught, recovery works

[test_oom] Tests complete, continuing boot...
```

The firmware continues to boot and run normally after catching the OOM.

---

## Problem 1: `#[panic_handler]` aborts instead of unwinding

### Symptom

`catch_unwind` never caught anything. Panics always reached the top-level panic handler,
printed a backtrace, and aborted.

### Root cause

In `no_std`, `panic!()` routes **directly** to `#[panic_handler]`. There is no automatic
unwinding step — the compiler does not insert `begin_panic` calls the way `std` does.

`esp-backtrace` (with `panic-handler` feature) provides a `#[panic_handler]` that prints
a backtrace and loops/aborts. It never calls `begin_panic`, so unwinding never starts.

The `unwinding` crate's `begin_panic` and `catch_unwind` are explicit APIs — they are
**not** compiler hooks. Unless something calls `begin_panic`, the unwinder is never invoked.

### Fix

Removed `panic-handler` feature from `esp-backtrace`. Wrote a custom `#[panic_handler]` that:

1. Prints panic info via `esp_println` (preserves debug output)
2. Calls `unwinding::panic::begin_panic()` to start stack unwinding
3. Falls through to `loop {}` if `begin_panic` returns (no `catch_unwind` on stack)

```rust
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    esp_println::println!("\n\n====================== PANIC ======================");
    esp_println::println!("{info}");
    esp_println::println!();

    struct PanicPayload; // ZST: no heap allocation during OOM
    let _code = unwinding::panic::begin_panic(Box::new(PanicPayload));

    esp_println::println!("unwinding failed or no catch_unwind on stack, aborting");
    loop {}
}
```

### Key insight

`begin_panic` returns `UnwindReasonCode`, not `!`. It returns if unwinding completes
without finding a `catch_unwind` frame. The fallthrough `loop {}` handles that case.

---

## Problem 2: Default alloc error handler uses nounwind panic

### Symptom

OOM panics were not catchable even with the custom panic handler wired up.

### Root cause

Since Rust 1.68 (PR #106045), the default `alloc_error_handler` calls `panic_nounwind_fmt`,
which is a non-unwinding panic. It bypasses the normal panic machinery entirely —
`catch_unwind` cannot intercept it.

### Fix

Define a custom `#[alloc_error_handler]` that calls regular `panic!()`:

```rust
#![feature(alloc_error_handler)]

#[alloc_error_handler]
fn on_alloc_error(layout: Layout) -> ! {
    panic!("memory allocation of {} bytes failed", layout.size());
}
```

This requires nightly (`alloc_error_handler` is unstable). The project already uses nightly.

---

## Problem 3: `.eh_frame` discarded by esp-hal linker scripts

### Symptom

After fixing problems 1 and 2, `begin_panic` was called but unwinding failed — it
couldn't walk the stack. The unwinder reported no matching `catch_unwind`.

### Root cause

The `unwinding` crate's `fde-static` feature reads `.eh_frame` sections from ROM to find
unwind tables. It needs three linker symbols:

- `__executable_start` — start of code region
- `__etext` — end of code region
- `__eh_frame` — start of `.eh_frame` data

esp-hal's `eh_frame.x` linker script places `.eh_frame` at **address 0** with `(INFO)` type:

```ld
SECTIONS {
  .eh_frame 0 (INFO) :
  {
    KEEP(*(.eh_frame));
  }
}
```

`(INFO)` means non-allocatable — the data is present in the ELF but **not loaded into
flash**. The `.eh_frame` section in the final binary was 0 bytes of useful data at address 0.
`__eh_frame` and `__etext` resolved to 0.

### Fix

The fw-esp32 `build.rs` patches esp-hal's linker scripts at build time:

1. **Replaces `text.x`** with a version that appends `.eh_frame` to the end of `.text`
2. **Replaces `eh_frame.x`** with a no-op comment

This works because esp-hal's `build.rs` runs first (it's a dependency), creating the
linker scripts. fw-esp32's `build.rs` runs second and overwrites them. The linker runs
last, using the patched versions.

```rust
// build.rs (simplified)
let patched_text = "
SECTIONS {
  .text : ALIGN(4) {
    KEEP(*(.init));
    KEEP(*(.init.rust));
    KEEP(*(.text.abort));
    *(.literal .text .literal.* .text.*)
    . = ALIGN(4);
    PROVIDE(__eh_frame = .);
    KEEP(*(.eh_frame));
    KEEP(*(.eh_frame.*));
  } > ROTEXT
}
";

// Patch ALL esp-hal-* build dirs (Cargo may use any depending on features)
for entry in read_dir(build_dir) {
    if name.starts_with("esp-hal-") {
        write(text_x, patched_text);
        write(eh_frame_x, "/* patched */");
    }
}
```

A supplemental linker script (`eh_frame_unwind.x`) provides the remaining symbols:

```ld
PROVIDE(__executable_start = ORIGIN(ROTEXT));
PROVIDE(__etext = ORIGIN(ROTEXT) + LENGTH(ROTEXT));
```

`__etext` is conservatively set to end-of-ROM rather than end-of-text. The unwinder uses
it only to validate that a PC falls within the binary — a wider range is harmless.

---

## Problem 4: LTO strips `.eh_frame` sections

### Symptom

Object files contained ~470 KB of `.eh_frame` data, but only 44 bytes survived into the
final binary.

### Root cause

Link-Time Optimization (LTO) merges all code into a single codegen unit. The LTO pass
strips `.eh_frame` sections unless the compiler is told to preserve them.

The workspace profile uses `lto = true` and `codegen-units = 1` for size optimization.
With `panic = "unwind"`, the compiler emits `.eh_frame` in pre-LTO objects, but LTO
discards it because the standard RISC-V bare-metal target doesn't expect unwind tables.

### Fix

Added `-C force-unwind-tables=yes` to rustflags in `.cargo/config.toml`:

```toml
rustflags = [
  "-C", "link-arg=-Tlinkall.x",
  "-C", "force-frame-pointers",
  "-C", "force-unwind-tables=yes",   # <-- prevents LTO from stripping .eh_frame
  "-C", "link-arg=--gc-sections",
  "-C", "link-arg=-znoseparate-code",
]
```

After this, the final binary contains ~244 KB of `.eh_frame` data.

---

## Problem 5: ESP32 bootloader 2-segment limit

### Symptom

Firmware failed to boot:

```
Assert failed in unpack_load_app, bootloader_utility.c:762 (rom_index < 2)
```

### Root cause

The ESP-IDF 2nd-stage bootloader (v5.1-beta1) maps flash-resident segments into the
ESP32-C6's cache/MMU address space. It supports at most **2 ROM-mapped segments**:

1. Read-only data (`.rodata`) — mapped as `R`
2. Code (`.text`) — mapped as `R E`

The bootloader iterates over ELF LOAD segments, counting those with virtual addresses in
the ROM region (`0x42000000–0x42800000`). If it finds more than 2, it asserts.

When `.eh_frame` was placed in its own section, it had different flags from `.text`
(no execute permission), so the linker created a 3rd LOAD segment:

```
LOAD  0x42000020  R     (.rodata)        ← segment 1 (ROM)
LOAD  0x420308f8  R E   (.text)          ← segment 2 (ROM)
LOAD  0x421725ec  R     (.eh_frame)      ← segment 3 (ROM) — EXCEEDS LIMIT
```

Several approaches failed:

- **`INSERT AFTER .text`** — lld created a second `.text` section at a different address
  rather than appending to the existing one.
- **Separate `.eh_frame` output section** — always gets different flags (`R` vs `R E`),
  always becomes a separate segment.
- **Appending to `.rodata`** — same problem: the second `.rodata` was placed after `.text`,
  creating a separate segment at a non-contiguous address.

### Fix

The `.eh_frame` input sections must be placed **inside** the same `.text` output section
definition. lld only merges input sections into one output section when they appear in the
same `SECTIONS { .text : { ... } }` block. A second `SECTIONS { .text : { ... } }` block
creates a second output section with the same name but potentially different attributes.

This is why the fix patches `text.x` directly (see Problem 3) — the `.eh_frame` KEEP
directives are inside the same `.text : { ... }` block as the code, so they inherit the
`AX` (alloc + execute) flags and share the same LOAD segment.

Result:

```
LOAD  0x42000020  R     (.rodata)        ← segment 1 (ROM)
LOAD  0x420308f8  R E   (.text + .eh_frame) ← segment 2 (ROM)
```

The `.text` section grew from ~1.25 MB to ~1.56 MB (the extra 244 KB is `.eh_frame`).

---

## Problem 6: Target spec overrides `panic = "unwind"` with `abort`

### Symptom

After all linker fixes, the unwinder walked the stack but returned code 5
(`_URC_END_OF_STACK`) — no catch frame found. `.gcc_except_table` sections (which contain
LSDA data with catch/cleanup clauses) were completely absent from the binary.

### Diagnosis

```bash
$ cargo rustc --target riscv32imac-unknown-none-elf --profile release-esp32 -- --print cfg | grep panic
panic="abort"
```

Despite `panic = "unwind"` in `[profile.release-esp32]`, the compiler was using `abort`.

### Root cause

The `riscv32imac-unknown-none-elf` target specification includes `"panic-strategy": "abort"`.
This target-level setting **overrides** the Cargo profile's `panic = "unwind"`. The profile
setting is silently ignored.

Without `panic = "unwind"` actually taking effect:
- The compiler emits `.eh_frame` (due to `-C force-unwind-tables=yes`) but no landing pads
- No `.gcc_except_table` / LSDA sections are generated
- The personality routine has no data to find catch handlers
- The unwinder walks every frame, finds no catch, returns `END_OF_STACK`

The `.eh_frame` alone only tells the unwinder how to restore registers for each frame.
The LSDA (in `.gcc_except_table`) tells it which frames have catch or cleanup handlers.
Without LSDA, the unwinder can walk the stack perfectly but has no idea where to stop.

### Fix

Add `-C panic=unwind` to rustflags in `.cargo/config.toml`. Compiler flags override the
target spec, unlike Cargo profile settings:

```toml
rustflags = [
  "-C", "panic=unwind",               # Override target's default panic=abort
  "-C", "force-frame-pointers",
  "-C", "force-unwind-tables=yes",
  # ...
]
```

After this change, the binary contains hundreds of `.gcc_except_table` sections with LSDA
data, and `cargo rustc -- --print cfg` shows `panic="unwind"`.

### Key insight

There are three layers that control panic strategy, with different precedence:

1. **Target spec** (`"panic-strategy": "abort"`) — lowest precedence for `-C` flags,
   but OVERRIDES Cargo profile
2. **Cargo profile** (`panic = "unwind"`) — overridden by target spec
3. **rustflags** (`-C panic=unwind`) — highest precedence, overrides everything

The Cargo profile's `panic` setting is only a *request*. If the target spec says `abort`,
the request is silently dropped. Only `-C panic=unwind` in rustflags forces it.

---

## Final file layout

### `lp-fw/fw-esp32/build.rs`

Patches esp-hal's `text.x` and `eh_frame.x` at build time. Scans all `esp-hal-*` build
directories (Cargo creates different ones for different feature combinations).

### `lp-fw/fw-esp32/linker/eh_frame_unwind.x`

Provides `__executable_start` and `__etext` symbols for the `unwinding` crate's
`fde-static` FDE lookup. Loaded via `cargo:rustc-link-arg=-T...` from `build.rs`.

### `lp-fw/fw-esp32/.cargo/config.toml`

- `-C panic=unwind` — overrides target spec's `panic-strategy: abort`
- `-C force-frame-pointers` — required for RISC-V stack walking
- `-C force-unwind-tables=yes` — preserves `.eh_frame` through LTO

### `lp-fw/fw-esp32/Cargo.toml`

- `esp-backtrace` without `panic-handler` feature (backtrace utility only)
- `unwinding` with `unwinder`, `fde-static`, `personality`, `panic`

### `lp-fw/fw-esp32/src/main.rs`

- Custom `#[panic_handler]` calling `begin_panic`
- Custom `#[alloc_error_handler]` using regular `panic!()`
- `test_oom` feature for on-device OOM recovery validation

### `Cargo.toml` (workspace)

- `[profile.release]` and `[profile.release-esp32]`: `panic = "unwind"`

---

## Binary size impact

| Section   | Before (panic=abort) | After (panic=unwind + .eh_frame + LSDA) | Delta |
|-----------|---------------------|------------------------------------------|-------|
| `.text`   | ~1.25 MB            | ~1.71 MB                                | +~460 KB (.eh_frame + landing pads) |
| `.rodata` | ~194 KB             | ~360 KB                                 | +~166 KB (.gcc_except_table LSDA) |
| Total ROM | ~1.44 MB            | ~2.05 MB                                | +~610 KB |

The increase is ~42% of ROM usage. With a 3 MB factory partition, this leaves ~975 KB
free. Tight but workable for a web UI.

---

## Debugging aid: `test_oom` feature

The `test_oom` feature (`just fwtest-oom-esp32c6`) runs a targeted OOM test at boot:

```rust
#[cfg(feature = "test_oom")]
{
    let result = unwinding::panic::catch_unwind(AssertUnwindSafe(|| {
        let mut vecs = Vec::new();
        loop { vecs.push(Vec::<u8>::with_capacity(64 * 1024)); }
    }));
    match result {
        Err(_) => log::info!("[test_oom] OOM caught successfully, recovery OK"),
        Ok(_)  => log::warn!("[test_oom] did not OOM"),
    }
}
```

This allocates 64 KB chunks in a loop until OOM, inside `catch_unwind`. If unwinding
works correctly, the OOM is caught, all `Vec` destructors run, heap is reclaimed, and
the firmware continues to boot normally.

---

## Lessons learned

1. **`no_std` panic flow is fundamentally different from `std`.** In `std`, `panic!()`
   calls `begin_panic` which starts unwinding. In `no_std`, `panic!()` calls
   `#[panic_handler]` directly. The `unwinding` crate does not hook into the compiler's
   panic machinery — you must call `begin_panic` yourself.

2. **Linker script ordering matters in non-obvious ways.** lld does not merge multiple
   `SECTIONS { .text : { ... } }` blocks into one output section. Each block creates a
   separate output section, potentially with different flags. To append content to an
   existing section, you must modify the original definition.

3. **ESP32 bootloader segment limits are undocumented and fatal.** The `rom_index < 2`
   assertion in `bootloader_utility.c` is not mentioned in ESP-IDF documentation. The
   bootloader simply refuses to load the app with no useful error message beyond the
   assert location.

4. **LTO interacts with unwind tables.** Even with `panic = "unwind"`, LTO may strip
   `.eh_frame` sections. `-C force-unwind-tables=yes` is required to override this.

5. **Build script patching is fragile but necessary.** Patching esp-hal's linker scripts
   at build time is a hack. It works because build script execution order is deterministic
   (dependencies before dependents) and the linker runs after all build scripts. But it
   will break if esp-hal changes its linker script names or structure. A future upstream
   contribution to esp-hal (or a linker script override mechanism) would be cleaner.

6. **Cargo profile `panic` is silently overridden by the target spec.** Setting
   `panic = "unwind"` in `[profile.release-esp32]` has no effect on
   `riscv32imac-unknown-none-elf` because the target spec hardcodes `abort`. Only
   `-C panic=unwind` in rustflags actually forces unwind mode. There is no warning.

7. **Pre-built core sysroot blocks unwinding through `panic!()`.** The pre-built
   `core` for `riscv32imac-unknown-none-elf` is compiled with `panic=abort`, so
   `core::panicking::panic()` and related functions lack proper unwind info for the
   unwinder to walk through. Direct calls to `unwinding::panic::begin_panic()` work
   because the entire call chain stays in user crates compiled with `panic=unwind`.
   To unwind through `panic!()`, `-Z build-std=core,alloc` is needed to rebuild core
   with unwind support.
