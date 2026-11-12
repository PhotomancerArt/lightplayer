# Phase 5: ESP Transport Integration

## Scope Of Phase

Use the streaming project-read response path on ESP so project-read responses no longer allocate a full serialized JSON `Vec<u8>` before serial write.

In scope:

- Route project-read requests on ESP/server path to the streaming writer.
- Keep existing full `WireServerMessage` send path for small messages such as heartbeat.
- Replace or bypass the misleading `VecWriter` full-buffer path for streamed project reads.
- Write `M!`, stream JSON, then write newline.
- Preserve host-client JSON compatibility.

Out of scope:

- Replacing all server messages with streaming writers.
- Binary/framed protocol redesign.
- Removing heartbeat/simple response paths.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

Relevant files:

```text
lp-fw/fw-esp32/src/transport.rs
lp-fw/fw-esp32/src/serial/io_task.rs
lp-app/lpa-server/src/...              # inspect actual project request handling path during implementation
lp-core/lpc-engine/src/engine/project_read_stream.rs
```

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

The current ESP path:

- Sends a full `WireServerMessage` through `OUTGOING_SERVER_MSG`.
- `io_task` serializes that full message into a heap `Vec<u8>`.
- Then `timed_write_all` writes the vector in chunks.

Target behavior for project read:

- Do not build a full serialized JSON vector.
- Use a writer adapter that writes small chunks to serial.
- If the semantic JSON writer is synchronous but serial write is async, use a bounded scratch writer that accumulates up to a small fixed limit and flushes asynchronously at safe boundaries. Keep the memory bound explicit and tested where possible.
- Keep the outer message JSON shape parseable by existing clients. If the outer `ServerMessage` envelope must still wrap `ProjectReadResponse`, stream that envelope manually too.

Important implementation constraint:

- Do not break the on-device compiler or feature-gate out core shader execution to make memory easier.

Tests/checks:

- Host tests for the writer path should already cover JSON equivalence.
- Add a firmware compile check.
- Run an ESP smoke if hardware is available.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire
cargo test -p lpc-engine project_read
cargo test -p lp-cli --no-run
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If hardware is available, also run the same demo command that triggered the OOM and confirm project reads no longer panic from full JSON response buffering.
