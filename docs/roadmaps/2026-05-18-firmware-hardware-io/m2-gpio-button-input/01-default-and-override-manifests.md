# Phase 1: Default And Override Manifests

## Scope Of Phase

In scope:

- Add a compiled default hardware manifest backed by checked-in TOML.
- Add ESP32 startup loading for `/hardware.toml` from the firmware filesystem.
- Refactor firmware startup order so the filesystem is mounted before hardware
  providers are constructed.
- Keep fallback behavior non-fatal.

Out of scope:

- Editing `/hardware.toml` over the wire.
- Dynamic LED pin dispatch.
- Button input implementation.
- Radio resources beyond preserving manifest vocabulary.

## Code Organization Reminders

- Prefer one clear concept per file.
- Put the shared compiled manifest helper in `lpc-shared::hardware`, not in an
  output provider.
- Put ESP32-specific filesystem override logic under `fw-esp32/src/hardware/`.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Add `lp-core/lpc-shared/src/hardware/default_manifests.rs`.

   Suggested public API:

   ```rust
   pub fn default_esp32c6_hardware_manifest() -> HardwareManifest
   ```

   It should use:

   ```rust
   const XIAO_ESP32_C6_TOML: &str =
       include_str!("../../boards/seeed/xiao-esp32-c6.toml");
   ```

   Parse through `HardwareManifestFile::read_toml(...).to_manifest()`.
   The default file is checked in, so panic/expect is acceptable for the
   compiled default path.

2. Re-export the default helper from `lp-core/lpc-shared/src/hardware/mod.rs`.

3. Add `lp-fw/fw-esp32/src/hardware/manifest_loader.rs`.

   Suggested API:

   ```rust
   pub fn load_hardware_manifest(fs: &dyn lpfs::LpFs) -> HardwareManifest
   ```

   Behavior:

   - Try reading `"/hardware.toml"`.
   - Missing file: return `default_esp32c6_hardware_manifest()`.
   - Read error other than missing: log warning and return default.
   - Parse/lower error: log warning and return default.
   - Success: return loaded manifest.

   Use `lpfs::lp_path::AsLpPath` to address `/hardware.toml`.

4. Add `lp-fw/fw-esp32/src/hardware/mod.rs` and expose the loader.

5. Update `lp-fw/fw-esp32/src/main.rs` normal server path:

   - Initialize board and RMT peripheral as today.
   - Mount flash or memory filesystem before creating `Esp32OutputProvider`.
   - Load hardware manifest from the mounted filesystem.
   - Defer provider construction to Phase 2 if needed, but make the manifest
     value available before provider construction.

6. Keep `lp-fw/fw-esp32/src/board/esp32c6/hardware_manifest.rs` only if needed
   as a compatibility wrapper. Prefer replacing its contents with a call to the
   shared default helper or removing it in cleanup.

## Tests To Add Or Update

- Unit test default helper parses the checked-in manifest and includes:
  - board id `seeed/xiao-esp32-c6`
  - `/gpio/18`
  - `/rmt/ws281x0`
  - reserved `/gpio/12`
- Unit test manifest loader behavior if practical with `LpFsMemory`:
  - missing `/hardware.toml` uses default
  - valid override uses override
  - invalid override falls back

If ESP32-only module tests are awkward, put the pure loader function in a shared
or host-testable module and keep ESP32 logging thin.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-shared hardware
cargo check -p lpc-shared --no-default-features
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

