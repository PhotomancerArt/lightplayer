# Phase 4: Engine Project Read Sink Path

## Scope Of Phase

Add an engine-side project-read path that writes response results/probes one at a time instead of collecting a full `ProjectReadResponse`.

In scope:

- Keep `Engine::read_project(request) -> ProjectReadResponse` unchanged for host/tests.
- Add a streaming/sink method that emits the same logical response through `ProjectReadResponseWriter` or a small trait.
- Add equivalence tests comparing streamed output to `Engine::read_project` for representative requests.
- Ensure resource payload streaming is used when resource payloads are included.

Out of scope:

- Rewriting slot snapshot generation internals.
- ESP transport integration.
- Removing any existing host APIs.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

Suggested files:

```text
lp-core/lpc-engine/src/engine/project_read_stream.rs
lp-core/lpc-engine/src/engine/mod.rs
lp-core/lpc-engine/src/engine/project_read_resources.rs
```

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Possible method shape:

```rust
impl Engine {
    pub fn write_project_read_json<W>(
        &self,
        request: ProjectReadRequest,
        writer: W,
    ) -> Result<(), ProjectReadStreamError<W::Error>>
    where
        W: JsonWrite,
    { ... }
}
```

Alternative: accept an already-created `ProjectReadResponseWriter<W>` if that keeps ownership simpler.

Behavior:

- Capture `revision = self.revision()` once at the start.
- Begin streamed response with that revision.
- Iterate `request.queries` and write each result immediately.
- Iterate `request.probes` and write each probe immediately.
- For resource results with payloads, use the streaming resource payload path rather than building encoded payload strings.
- It is acceptable for the first version to build each individual `ProjectReadResult` as an owned struct except for resource payload byte encoding.

Tests:

- A default debug request streamed from engine deserializes to the same response as `read_project`.
- A resource-payload request streamed from engine deserializes to the same response as `read_project`.
- A request with probes, if test support exists, preserves probe order and shape.
- A chunk-counting writer shows the streamed path performs multiple bounded writes for a response that would otherwise be large.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine project_read
cargo test -p lpc-engine
cargo test -p lpc-wire
```
