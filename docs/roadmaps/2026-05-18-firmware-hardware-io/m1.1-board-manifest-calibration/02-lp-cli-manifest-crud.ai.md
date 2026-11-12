# Phase 2: lp-cli Manifest CRUD

## Scope Of Phase

Add developer-facing `lp-cli hardware manifest` commands for discovering, listing, showing, creating,
validating, and deleting checked-in board manifests. The default command should be interactive and
should not require flags for the normal human workflow.

Out of scope: physical GPIO calibration, flashing firmware, and packaged/user-facing tooling.

## Code Organization Reminders

- Prefer one clear concept per file.
- Keep command parsing in `args.rs`, orchestration in `handler.rs`, and store/file operations in
  manifest-specific modules.
- Put tests at the bottom of files or in focused integration tests.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add `lp-cli/src/commands/hardware/`:

- `args.rs` with `HardwareCli`, `HardwareSubcommand`, `ManifestSubcommand`.
- `handler.rs` dispatching subcommands.
- `manifest/board_manifest_store.rs` that finds the repo root and resolves
  `lp-core/lpc-shared/boards`.
- `manifest/board_manifest_commands.rs` for command behavior.

Wire into:

- `lp-cli/src/commands/mod.rs`
- `lp-cli/src/main.rs`

Target commands:

```bash
lp-cli hardware manifest
lp-cli hardware manifest list
lp-cli hardware manifest show seeed/xiao-esp32-c6
lp-cli hardware manifest validate
lp-cli hardware manifest validate seeed/xiao-esp32-c6
lp-cli hardware manifest new --target esp32c6 --vendor seeed --product "XIAO ESP32-C6" --url https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html
lp-cli hardware manifest delete seeed/xiao-esp32-c6
```

Implementation constraints:

- Default to the repository root. Allow `--repo <path>` or `--boards-dir <path>` if easy and useful
  for tests.
- `lp-cli hardware manifest` with no subcommand should show an interactive list of existing
  manifests and an option to add a new one.
- The interactive "add new manifest" path should prompt for target, vendor, product, URL, and
  optional description. It should offer supported targets as a menu, not require typing the enum.
- The interactive selected-manifest path should offer show, edit metadata, validate, delete, and
  return/back actions.
- Reject ids containing `..`, absolute paths, empty path segments, or unsafe filename characters.
- `new` should refuse to overwrite an existing manifest unless a deliberate `--force` exists.
- `new` should require `--target` and validate that it is one of the supported hardware targets.
- `delete` should require confirmation via `dialoguer` unless `--yes` is passed.
- `show` should print high-signal manifest metadata and resource counts.
- `validate` should print all invalid files and exit non-zero if any fail.

## Validate

```bash
cargo test -p lp-cli hardware
cargo check -p lp-cli
cargo run -p lp-cli -- hardware manifest
cargo run -p lp-cli -- hardware manifest list
cargo run -p lp-cli -- hardware manifest show seeed/xiao-esp32-c6
cargo run -p lp-cli -- hardware manifest validate
```
