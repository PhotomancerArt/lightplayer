# Phase 3: Manifest Writeback And Resume State

## Scope Of Phase

Persist confirmed calibration results while keeping interrupted or unconfirmed state out of source
manifests.

In scope:

- Add calibration resume/session-state files under `target/hardware-calibration/`.
- Write confirmed GPIO label mappings back to `HardwareManifestFile`.
- Record confirmed dangerous pins with `reserved_reason`.
- Preserve useful aliases when replacing provisional `display_label` values.
- Add tests around manifest updates and resume serialization.

Out of scope:

- Comment-preserving TOML edits.
- A database or global user config store.
- Multi-board batch calibration.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-cli/src/commands/hardware/calibrate/calibration_session.rs`
- new `lp-cli/src/commands/hardware/calibrate/calibration_manifest_update.rs`
- new `lp-cli/src/commands/hardware/calibrate/calibration_resume.rs`
- `lp-cli/src/commands/hardware/manifest/board_manifest_store.rs`
- `lp-core/lpc-shared/src/hardware/hardware_manifest_file.rs`
- `lp-core/lpc-shared/boards/seeed/xiao-esp32-c6.toml`

Resume state:

- Store under `target/hardware-calibration/`.
- Derive filename from manifest id, for example `seeed__xiao-esp32-c6.json` or `.toml`.
- Include:
  - board id
  - target
  - physical label being probed
  - current candidate index
  - confirmed mappings not yet written
  - crash-suspect pins and whether the user confirmed them

Manifest update behavior:

- For a confirmed mapping, find the resource by internal address `/gpio/<n>`.
- Set `display_label` to the board-visible label the user entered.
- Preserve previous `display_label` as an alias if it is non-empty and not already present.
- Preserve existing aliases.
- For a confirmed dangerous pin, set:

```toml
reserved_reason = "crashed or timed out during calibration"
```

- If a GPIO resource is missing from the manifest, add it with:
  - `address = "/gpio/<n>"`
  - `display_label = <entered board label>` for confirmed mapping, or `GPIO<n>` for dangerous-only
  - GPIO input/output capabilities
  - useful aliases such as `GPIO<n>` and `IO<n>`

User confirmation:

- After `y`, show the mapping and ask for final confirmation only if needed. The happy path should
  stay lightweight.
- On crash/timeout, show a warning like:

```text
GPIO12 stopped responding within 1000ms. This may be a dangerous pin on this board.
Mark /gpio/12 as dangerous and skip it in future calibration? (y/N)
```

Validation:

- Test that a label update preserves old label as alias.
- Test that dangerous pins get `reserved_reason`.
- Test that resume files round-trip.
- Test that source manifests are not written on quit unless confirmed changes are being saved.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli hardware
cargo test -p lpc-shared hardware
cargo run -p lp-cli -- hardware manifest validate
```
