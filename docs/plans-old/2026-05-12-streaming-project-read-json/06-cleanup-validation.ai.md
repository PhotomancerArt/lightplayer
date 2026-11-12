# Phase 6: Cleanup And Validation

## Scope Of Phase

Clean up the plan implementation, documentation, comments, and validation after the streaming response path is working.

In scope:

- Remove misleading comments that claim streaming while buffering full JSON.
- Ensure new writer APIs have concise rustdocs explaining memory behavior and limitations.
- Remove temporary TODOs or convert them to explicit future-work notes only where warranted.
- Run final validation commands.
- Check for accidental full-response/full-JSON buffering in the new ESP project-read path.

Out of scope:

- Adding new features beyond cleanup.
- Replacing JSON with a new protocol.
- Streaming every message type.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO` only if it is genuinely future work.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Checklist:

- `rg "never buffers full JSON|VecWriter|to_string\(|to_vec\(" lp-fw/fw-esp32 lp-core/lpc-wire lp-core/lpc-engine` and inspect relevant hits.
- Ensure the remaining full-buffer paths are either host-only, small-message-only, or documented as not used for large project-read responses.
- Ensure streamed JSON tests compare semantic deserialization, not brittle string formatting, except where exact punctuation/escaping tests are useful.
- Ensure base64 tests cover padding and non-multiple-of-three lengths.
- Update plan notes or future work if implementation discovers limits.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire
cargo test -p lpc-engine project_read
cargo test -p lpc-view
cargo test -p lp-cli --no-run
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

If practical before pushing:

```bash
just check
```

Run hardware smoke if available and record whether the previous OOM reproduction still occurs.
