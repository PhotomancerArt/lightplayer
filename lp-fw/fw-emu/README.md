# fw-emu

`fw-emu` is the RV32 firmware image used by the LightPlayer emulator tests.

It preserves the embedded shape of the product while running under the
repository's RISC-V emulator infrastructure. This makes it possible to validate
real firmware behavior, shader compilation, server behavior, and panic recovery
without requiring physical ESP32 hardware for every test.

## Relationship To Other Crates

- `fw-core` provides shared firmware transport/logging plumbing and runtime loop
  helpers with the `emu` feature.
- `lpa-server` runs inside the firmware image.
- `lp-riscv-emu`, `lp-riscv-emu-guest`, and related crates provide the emulator
  host/guest infrastructure.
- `fw-tests` contains host-side tests that build and exercise this firmware.

`fw-emu` is not a host runtime like `fw-host`; it is still firmware, just running
inside the emulator.

## Validation

Build/check the emulator firmware:

```bash
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

Run firmware emulator tests that exercise real shader compilation and execution:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
```

Do not use host workspace-wide cargo commands for this target. Use the targeted
commands or root just recipes described in `AGENTS.md`.
