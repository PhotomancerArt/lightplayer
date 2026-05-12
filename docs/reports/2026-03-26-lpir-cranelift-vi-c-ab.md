# LPIR / Cranelift Stage VI-C — A/B and validation report

**Plan:** [Stage VI-C design](../plans-done/2026-03-25-lpvm-cranelift-stage-vi-c/00-design.md) (
moved to `plans-done` when the stage completed).  
**Roadmap:** [stage-vi-c-esp32.md](../roadmaps-old/2026-03-24-lpvm-cranelift/stage-vi-c-esp32.md)

## Purpose

Document automated validation for the **lp-server → lp-engine → lpvm-cranelift** path on **fw-emu**
and **fw-esp32**, and reserve space for **old-vs-new compiler** comparisons when two worktrees are
available. Primary quantitative story is **memory / allocation behavior on fw-emu**; ESP32 is for
correctness, size, and integration.

## Environments (this run)

| Field          | Value                                                                                                                  |
|----------------|------------------------------------------------------------------------------------------------------------------------|
| Date           | 2026-03-26                                                                                                             |
| Host           | macOS (darwin), local dev machine                                                                                      |
| `rustc`        | 1.96.0-nightly (2026-03-12)                                                                                            |
| Branch / SHA   | `feature/lpvm-cranelift` — record `git rev-parse --short HEAD` when reproducing                                        |
| “New” compiler | `lpvm-cranelift` (transitive from `lp-engine`)                                                                         |
| “Old” compiler | Not re-measured in this session — compare when a checkout with `lps-cranelift` on the firmware path is still available |

## fw-emu gate (automated)

Commands run; all **passed** on 2026-03-26:

```bash
just build-fw-emu
cargo test -p fw-tests
cargo test -p lpa-client --features serial --test scene_render_emu_async
```

Notes:

- `scene_render_emu_async` is marked `#[ignore]` but **must compile** with `--features serial` (
  serial transport + emulator helpers). The `lp-client` crate now declares
  `required-features = ["serial"]` for that test target so default `cargo test -p lp-client` does
  not fail on missing symbols.
- Emulator-heavy `fw-tests` targets: `scene_render_emu`, `alloc_trace_emu`, `unwind_emu`.

## Memory (primary signal — fw-emu)

| Metric                                      | Method                                                      | This worktree (new path)      | Old worktree | Notes                                                                   |
|---------------------------------------------|-------------------------------------------------------------|-------------------------------|--------------|-------------------------------------------------------------------------|
| Alloc trace events (scene load + short run) | `cargo test -p fw-tests --test alloc_trace_emu`             | **15008** events (2026-03-26) | TBD          | From test log: “Trace produced … events”. Repeat on old tree for delta. |
| Deeper heap / peak RSS                      | `lp-cli` `mem_profile` (builds `fw-emu` with `alloc-trace`) | Not run in this session       | TBD          | See `lp-cli/src/commands/mem_profile/handler.rs`.                       |

## Compile time (fw-emu / host)

| Step           | Command             | Wall time (indicative)    | Notes                                          |
|----------------|---------------------|---------------------------|------------------------------------------------|
| Release fw-emu | `just build-fw-emu` | ~23 s (clean-ish rebuild) | Single sample; use `time` / hyperfine for A/B. |

## Execution / frame time

| Scenario                            | Result | Notes                                   |
|-------------------------------------|--------|-----------------------------------------|
| `fw-tests` scene render in emulator | Pass   | WS2811 output writes exercised          |
| Shader compile in emu               | Pass   | Log: “Shader 1 compiled” in scene tests |

## Firmware binary size (fw-esp32)

| Artifact       | Path pattern                                                 | Size (bytes)  | Date       |
|----------------|--------------------------------------------------------------|---------------|------------|
| `fw-esp32` ELF | `target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32` | **1_163_820** | 2026-03-26 |

Compare with an old worktree after `just build-fw-esp32` with the same profile/features.

## Automated matrix (additional)

```bash
cargo test -p lp-engine
cargo test -p lpa-server
cargo test -p lpvm-cranelift
cargo test -p lpvm-cranelift --no-default-features   # options + q32 encode only; host JIT tests require `std`
cargo test -p lpvm-cranelift --features riscv32-emu
cargo clippy -p lp-engine -p lpa-server -p lpvm-cranelift -p lpa-client --all-features -- -D warnings
just build-fw-esp32
```

## Manual ESP32 checklist (owner — TBD)

- [ ] Flash `fw-esp32` (`release-esp32`, `esp32c6` + `server` as needed).
- [ ] Load project; confirm shaders compile on device.
- [ ] Visual output matches expectation.
- [ ] OOM / stability smoke (no persistent crash under normal scene).
- [ ] Note serial / timing quirks.
- [ ] Optional: flash size vs table above if bootloader/partitions changed.

## Known issues / follow-ups

- **Host JIT vs `--no-default-features`:** Without `std`, `lpvm-cranelift` JIT targets RISC-V;
  executing that code on the host is invalid. Host JIT integration tests are gated behind
  `feature = "std"`; `tests_options` runs under `--no-default-features`.
- **fw-emu / `lp-engine` no-std:** `render` reports JIT unavailable without `std` (expected on
  `fw-emu` firmware build).
- **Old-vs-new numbers:** Fill memory and compile tables when a second checkout is available.
