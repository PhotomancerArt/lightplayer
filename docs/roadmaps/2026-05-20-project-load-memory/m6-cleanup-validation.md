# M6: Cleanup And Validation

## Goal

Turn the memory reductions into a durable regression guard instead of a one-time
debug win.

## Work

- Re-run project-load profiles for `examples/basic` and `examples/button-sign`
  after each structural milestone.
- Capture ESP32-C6 serial memory checkpoints for the same build/profile.
- Add documentation for the expected memory budget and profile command.
- Add CI or local validation hooks where they are stable and cheap enough.
- Remove temporary logging once equivalent profile events exist.

## Deliverables

- Final before/after memory table.
- Updated profiling docs.
- A recommendation for the next memory budget target if compile still fails.

## Validation

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

## Implementation Strategy

Small plan. This milestone is mostly measurement, docs, and making sure the
team has a repeatable way to keep the heap from creeping back up.
