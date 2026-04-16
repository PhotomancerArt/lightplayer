# Phase 3: Integration Test

## Scope

Add an integration test in `fw-tests` that builds fw-emu with `alloc-trace`,
runs a project load cycle in the emulator, and verifies that the trace output
is produced and contains valid data.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update fw-emu to forward the alloc-trace feature

In `lp-fw/fw-emu/Cargo.toml`, add:

```toml
[features]
alloc-trace = ["lp-riscv-emu-guest/alloc-trace"]
```

This allows building fw-emu with `--features alloc-trace` to enable the
tracking allocator in the guest.

### 2. Add integration test

Create or extend a test in `lp-fw/fw-tests/tests/` (e.g. `alloc_trace_emu.rs`
or add to `scene_render_emu.rs`). Prefer a new file to keep it focused.

The test should:

1. Build fw-emu with `alloc-trace` feature + backtrace support:
   ```rust
   let fw_emu_path = ensure_binary_built(
       BinaryBuildConfig::new("fw-emu")
           .with_target("riscv32imac-unknown-none-elf")
           .with_profile("release-emu")
           .with_backtrace_support(true)
           .with_features(&["alloc-trace"]),
   ).expect("Failed to build fw-emu");
   ```

   Note: `BinaryBuildConfig::with_features()` may need to be added to
   `test_util.rs` if it doesn't exist. Check first.

2. Load ELF, extract symbol list, create trace output dir (use `tempdir`).

3. Create emulator with alloc tracing enabled:
   ```rust
   let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
       .with_alloc_trace(&trace_dir, metadata)?
       // ... other config ...
   ```

4. Set up serial transport, create client, build a simple project with
   `ProjectBuilder` (same pattern as `scene_render_emu`).

5. Sync files, load project, tick a few frames, unload project.

6. Shut down emulator, flush tracer.

7. Assert:
   - `meta.json` exists and is valid JSON
   - `meta.json` contains a non-empty `symbols` array
   - `heap-trace.jsonl` exists and is non-empty
   - Each line in `heap-trace.jsonl` is valid JSON
   - There are both `A` and `D` events
   - Events have non-empty `frames` arrays
   - `ic` values are monotonically non-decreasing

### 3. Add `with_features` to BinaryBuildConfig if needed

Check `lp-riscv/lp-riscv-emu/src/test_util.rs` for `BinaryBuildConfig`. If
there's no `with_features()` method, add one:

```rust
pub fn with_features(mut self, features: &[&str]) -> Self {
    self.features = features.iter().map(|s| s.to_string()).collect();
    self
}
```

And update `ensure_binary_built` to pass `--features` to cargo when building.

## Validate

```bash
# Run the new integration test
cargo test -p fw-tests test_alloc_trace -- --nocapture

# Run existing tests still pass
cargo test -p fw-tests test_scene_render_fw_emu -- --nocapture
```

Note: these tests build the fw-emu binary, which takes time. Use `--nocapture`
to see build progress.
