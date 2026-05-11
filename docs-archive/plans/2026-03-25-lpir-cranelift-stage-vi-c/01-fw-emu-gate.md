# Phase 1: fw-emu gate (build + smoke)

## Scope of phase

Prove the RISC-V firmware-in-emulator path is healthy **before** any ESP32 flash.
This is the dependency agreed in Q4.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **Release build** for the guest firmware (workspace root):

   ```bash
   just build-fw-emu
   ```

   Equivalent:

   ```bash
   rustup target add riscv32imac-unknown-none-elf   # if needed
   cargo build --target riscv32imac-unknown-none-elf -p fw-emu --release
   ```

2. **Integration smoke** (builds `fw-emu` as part of the test harness):

   - From repo root, run the firmware test crate(s) that exercise the emulator
     with a real ELF, for example:

   ```bash
   cargo test -p fw-tests
   ```

   Include scene-render coverage if not already implied:

   ```bash
   cargo test -p lpa-client --features serial --test scene_render_emu_async
   ```

   Adjust flags if your workflow uses `--features` on these crates.

3. If anything fails, **fix on this branch** before Phase 2 (`fw-esp32` manifest
   cleanup) or document a blocking issue in `00-notes.md` / the report.

4. **No code changes required** for this phase if everything is already green —
   still record pass/fail and exact commands in the A/B report template when you
   create it in Phase 4.

## Validate

```bash
just build-fw-emu
cargo test -p fw-tests
cargo test -p lpa-client --features serial --test scene_render_emu_async
```

Add `cargo build --target riscv32imac-unknown-none-elf -p fw-emu --release`
explicitly in CI notes if `just` is not used.
