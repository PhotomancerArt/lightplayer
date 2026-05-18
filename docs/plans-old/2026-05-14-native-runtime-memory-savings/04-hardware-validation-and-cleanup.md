# Phase 4: Hardware Validation and Cleanup

## Scope of phase

Re-run the ESP32 stress harness, collect final before/after memory numbers, and clean up any temporary instrumentation or stray profiling artifacts that are no longer useful.

In scope:
- run the one-shot stress harness on device
- compare peak heap and notable stage behavior to the previous baseline
- clean temporary debug prints or TODOs if any were introduced
- write summary notes for the completed plan

Out of scope:
- new architectural work beyond small cleanup fixes discovered during validation

## Code organization reminders

- Prefer granular files with one main concept per file
- Keep related functionality grouped together
- Put helpers lower in the file when that improves readability
- Mark any temporary code with a clear `TODO`

## Sub-agent reminders

- Do not commit
- Do not expand scope
- Do not suppress warnings or weaken tests to get green builds
- If blocked, stop and report instead of improvising
- Report what changed, what was validated, and any deviations

## Implementation details

Relevant files and symbols:
- `justfile`
- `scripts/run_until_marker.py`
- `lp-fw/fw-esp32/src/tests/incremental_shader_compile/*`
- plan docs under `docs/plans/2026-05-14-native-runtime-memory-savings/`

Expected changes:
- Keep the harness usable and ergonomic.
- Preserve any instrumentation that remains genuinely useful; remove only noise.
- Record final measurements in the plan summary.

## Validate

Run:

```bash
just fwtest-shader-compile-stress-trace-esp32c6
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```
