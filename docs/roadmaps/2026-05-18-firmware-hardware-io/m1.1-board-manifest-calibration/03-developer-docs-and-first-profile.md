# Phase 3: Developer Docs And First Profile

## Scope Of Phase

Update docs so `lp-cli` is described as a developer-facing repository tool, and make sure the first
Seeed XIAO ESP32-C6 manifest can be created or maintained through the new CRUD workflow.

Out of scope: splitting CLI binaries and building user-facing packaging.

## Code Organization Reminders

- Keep README changes concise and accurate.
- Do not turn docs into a roadmap dump; link to roadmap notes for deeper context.
- Keep generated or tool-created manifest formatting stable.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update:

- `README.md`
- `docs/architecture.md`

Document:

- `lp-cli` is currently developer-facing.
- It is designed to run from the repository/codebase.
- It includes server/dev/debug/profile/hardware manifest workflows.
- A future split may separate deployable/user-facing tools from internal developer tools.

Use the new tool to confirm the first manifest:

```bash
cargo run -p lp-cli -- hardware manifest
cargo run -p lp-cli -- hardware manifest show seeed/xiao-esp32-c6
cargo run -p lp-cli -- hardware manifest validate seeed/xiao-esp32-c6
```

If the manifest file is not present yet, create it with:

```bash
cargo run -p lp-cli -- hardware manifest new \
  --target esp32c6 \
  --vendor seeed \
  --product "XIAO ESP32-C6" \
  --url https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html
```

## Validate

```bash
cargo check -p lp-cli
cargo run -p lp-cli -- hardware manifest
cargo run -p lp-cli -- hardware manifest list
cargo run -p lp-cli -- hardware manifest show seeed/xiao-esp32-c6
cargo run -p lp-cli -- hardware manifest validate
```
