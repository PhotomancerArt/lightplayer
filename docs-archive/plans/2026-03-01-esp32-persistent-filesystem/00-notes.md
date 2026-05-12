# Notes: ESP32 Persistent Filesystem

## Scope of Work

Implement a persistent flash-backed filesystem on fw-esp32 so that **projects pushed via `just demo-esp32c6-host` survive device reboots**.

**Principal goal**: After running `just demo-esp32c6-host`, power-cycling the ESP32-C6, and rebooting, the project should still be present and loadable on the device (ideally auto-loading so the display works without reconnecting the host).

**In scope**:
- Flash-backed littlefs2 filesystem in a dedicated partition (1MB)
- `LpFs` implementation backed by littlefs2 (replacing or alongside `LpFsMemory`)
- Partition table configuration for esp-bootloader
- Boot behavior: load persisted project(s) from flash (lexical first; optional config file)
- Remove `demo_project` feature from fw-esp32
- lp-cli: new `upload` command – push project to host/device and exit (non-interactive)
- New just command: flash firmware → lp-cli upload examples/basic serial:auto

**Out of scope** (for this plan):
- OTA / firmware update partitions
- lp-cli handling firmware flashing (future)
- Encryption of flash storage

## Current State of the Codebase

### demo-esp32c6-host Flow
- Builds and flashes fw-esp32 with `esp32c6` + `server` features
- Runs `lp-cli dev examples/basic --push serial:auto`
- Host pushes project files to device via serial (FsRequest::Write)
- Device stores in `LpFsMemory` (RAM) → **lost on reboot**

### fw-esp32 Main
- Uses `LpFsMemory::new()` as `base_fs` for `LpServer`
- `LpServer` expects `Box<dyn LpFs>` – any `LpFs` implementation works
- No `esp-storage`, `littlefs2`, or flash partition setup

### LpFs Trait (lp-shared)
- `read_file`, `write_file`, `file_exists`, `is_dir`, `list_dir`, `delete_file`, `delete_dir`
- `chroot` → returns `Rc<RefCell<dyn LpFs>>` (can use `LpFsView` to wrap)
- `current_version`, `get_changes_since`, `clear_changes_before`, `record_changes` – used for hot-reload; flash backend can use simple in-RAM change tracking on write

### Prior Plan (Deferred)
- `docs/plans-done/26-01-05-initial-firmware/09-implement-esp32-filesystem.md`
- Proposed esp-storage + littlefs2, 1MB at 0x300000
- Never implemented; deferred in favor of `LpFsMemory` for initial bring-up
- Trait was `lp-core::traits::Filesystem` – now `LpFs` in lp-shared

### Dependencies
- fw-esp32: esp-hal, esp-bootloader-esp-idf, lp-shared (no std). No esp-storage or littlefs2.
- littlefs2 uses C bindings (littlefs2-sys), `c-stubs` feature for no_std

---

## Questions That Need Answers

### Q1: Partition size and placement ✅ **ANSWERED**
**Context**: User requested ~100KB initially. Re-evaluated given app size.

**Answer**: lpfs 1MB, app ~3MB. No OTA for now.

**Layout** (4MB flash):
- Bootloader, partition table, nvs, phy: ~64KB at start
- App: ~3MB at 0x10000 (exact size TBD to fit; app is ~1.5–2MB today)
- lpfs: 1MB immediately after app
- Example: app 0x10000 size 0x300000 (3MB), lpfs 0x310000 size 0x100000 (1MB)
- Need custom partition table CSV; esp-bootloader-esp-idf must be configured to use it

### Q2: Boot behavior – auto-load or wait for host ✅ **ANSWERED**
**Context**: For "persistent after reboot", we need to decide what happens on boot when projects exist in flash.

**Answer**:
- **Default (no config)**: Load first project by lexical order.
- **Future / optional now**: Config file in fs, writable by host, e.g. `{"auto_load": "project-name"}`. If present, use it; else lexical first.
- Nice to have in this plan if not too hard; not critical.

**Implementation note**: Config file `lightplayer.json` at fs root. General app config for now; may be split later. Struct in lp-model: `LightplayerConfig { startup_project: Option<String> }`. Host can write via FsRequest::Write.

### Q3: demo_project feature interaction ✅ **ANSWERED**
**Context**: `demo_project` embeds a test project and populates LpFsMemory on boot. With flash FS, we have two sources: embedded demo vs. persisted projects.

**Answer**: Remove demo_project. lp-cli sends projects. No embedded demo.

**Related**:
- New `lp-cli upload` command: push project to host and exit (no fs_loop, no UI)
- New just command: flash firmware → lp-cli upload examples/basic serial:auto
- Eventually: lp-cli could handle firmware flashing too

### Q4: Fallback when flash init fails ✅ **ANSWERED**
**Context**: Flash could be corrupted or partition missing.

**Answer**: Fall back to `LpFsMemory` with a warning. Device still usable; host can re-push project.

### Q5: Host tools for testing ✅ **ANSWERED**
**Context**: Need to create littlefs images on host for tests and for pre-flashing initial content?

**Answer**: No – firmware initializes/formats the partition on first mount. lp-cli pushes projects via serial (FsRequest::Write); firmware writes to littlefs. No host-side littlefs tool needed for main workflow.

**Optional later**: Host tools (littlefs-python, etc.) for debugging (inspect unpacked partition) or if we add tests that verify flash contents. Out of scope for this plan.

### Q6: Feature flag strategy ✅ **ANSWERED**
**Context**: Flash FS adds dependencies (esp-storage, littlefs2) and partition table setup. May want firmware without persistence for testing.

**Answer**:
- Add `flash_fs` feature – when enabled, try flash, fall back to LpFsMemory on init failure; when disabled, use LpFsMemory only (no flash attempt).
- Allows builds without persistence for testing; fallback when persistence is enabled but fails.

---

## Notes

- **lightplayer.json**: General app config. `LightplayerConfig { startup_project: Option<String> }` in lp-model. Path: `/lightplayer.json` at fs root. May be split later.
