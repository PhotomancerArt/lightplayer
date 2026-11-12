# Stage VI-C: ESP32 Hardware Validation

## Goal

Run `fw-esp32` on real hardware with the new `lpvm-cranelift` compiler (already
wired through `lp-engine` in Stage VI-B). Validate shaders render correctly on
device. Run A/B performance comparisons against the old compiler.

## Suggested plan name

`lpvm-cranelift-stage-vi-c`

## Scope

**In scope:**

- **ESP32 firmware build:**
    - Update `fw-esp32` Cargo.toml: replace `lps-cranelift` (+ direct
      cranelift crate deps) with `lpvm-cranelift` (without default `std` feature)
    - Resolve any remaining `no_std` / linker / ISA issues specific to
      ESP32-C6 (`riscv32imac`)
    - Build and flash `fw-esp32`
- **On-device validation:**
    - Shaders compile and render correctly
    - Monitor compilation memory usage on device (peak heap)
    - Verify no OOM during shader compilation
- **A/B performance comparison:**
    - Git worktree on main with old compiler
    - Compare: binary size (firmware image), compilation memory peak,
      compilation time, shader execution speed (frame time)
    - Document results
- **Triage any hardware-specific issues** not caught by `fw-emu`
  (timing, memory fragmentation, real serial I/O)

**Out of scope:**

- lp-engine migration (done in Stage VI-B)
- `lpvm-cranelift` embedded readiness (done in Stage VI-A)
- Optimization of the LPIR path (if slower, document and defer)
- Native f32 mode, Q32 wrapping mode
- Old compiler deletion (Stage VII)

## Deliverables

- `fw-esp32` building and running shaders with new compiler
- A/B comparison document: binary size, memory, compilation time,
  execution speed
- Known issues list (regressions, if any)

## Dependencies

- Stage VI-B (lp-engine migrated, fw-emu validated)
- Stage VI-A (lpvm-cranelift embedded readiness)

## Estimated scope

~100–200 lines of `fw-esp32` Cargo.toml + build fixes. Most of the work
is testing on hardware and writing up the A/B comparison.
