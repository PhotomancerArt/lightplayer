# Milestone 4: Performance Validation + Cleanup

## Goal

Validate fastalloc performance against cranelift, clean up temporary
scaffolding, update documentation, and ensure the allocator is production-ready.

## Suggested plan name

`fastalloc-m4`

## Scope

### In scope

- Run perf filetests, compare instruction counts (target: <1.3x vs cranelift,
  down from 1.88x)
- Run on-device benchmark, compare FPS (target: >27 FPS, closing the gap from
  25 → 29)
- Profile compile time impact on ESP32 (fastalloc vs greedy)
- Make fastalloc the default allocator (`USE_FASTALLOC = true`)
- Remove the old emitter path (static-map consumption) if no longer needed
- Remove `emit_call_preserves_before` / `emit_call_preserves_after` /
  `regs_saved_for_call` from `emit.rs`
- Update `docs/design/lpvm/notes.md` with final performance comparison
- Update `docs/design/native/2026-04-09-fastalloc-mini.md` with implementation
  notes
- Filetest expected-instruction-count updates for improved codegen

### Out of scope

- Removing greedy/linear scan allocators (kept as fallbacks)
- Param-to-callee-saved optimization (future improvement, tracked in roadmap
  notes)

## Key Decisions

- Greedy and linear scan allocators are kept behind config flags as validation
  fallbacks. They can be removed in a future cleanup if desired.
- Performance targets are aspirational — if fastalloc achieves correctness but
  falls short on perf, that's still a success and the gap is documented for
  future optimization.

## Deliverables

- Performance comparison table (filetests + on-device FPS)
- `config.rs`: `USE_FASTALLOC = true` as default
- `emit.rs`: old call-save machinery removed
- Updated design docs and notes
- Updated filetest expected values
- Validation commands documented:
  ```bash
  # Filetests
  scripts/glsl-filetests.sh --target rv32,rv32lp lpvm/native/perf

  # Emulator tests
  cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu

  # ESP32 build
  cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
    --profile release-esp32 --features esp32c6,server
  ```

## Dependencies

- M3 (control flow support — all shaders compile correctly)

## Estimated Scope

~100-150 lines of code changes (mostly deletions of old call-save code).
Primary effort is benchmarking, documentation, and verification.
