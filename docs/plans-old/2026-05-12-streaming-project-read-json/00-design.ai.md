# Design: Streaming Project Read JSON

## Scope Of Work

Add a bounded-memory JSON response path for project reads, aimed first at ESP32 server responses.

The design keeps the current JSON shape and semantic message structs, but avoids requiring embedded firmware to construct and serialize an entire `ProjectReadResponse` at once.

In scope:

- A small semantic JSON writer in `lpc-wire`.
- A streaming base64 helper for binary payload fields.
- A streamed `ProjectReadResponse` envelope writer that preserves the current JSON shape.
- An engine-side project-read sink path that can produce results/probes one at a time.
- ESP transport integration for project-read responses.
- Tests proving streamed JSON deserializes to the same semantic structs as normal serde JSON.

Out of scope:

- Replacing JSON with a binary protocol.
- Rewriting all message types.
- Removing normal `ProjectReadResponse` construction for host/tests.
- Streaming every slot-data subtree internally.
- Solving incremental slot diffing.

## File Structure

```text
lp-core/lpc-wire/src/
  json.rs                         # keep facade; may re-export writer modules
  json/
    json_write.rs                  # no_std-friendly write trait + alloc test adapters
    json_writer.rs                 # semantic object/array/property writer
    streaming_base64.rs            # base64 string writer for byte slices/chunks
  messages/project_read/
    stream_response.rs             # streamed ProjectReadResponse envelope helpers
    mod.rs                         # exports stream helpers

lp-core/lpc-engine/src/engine/
  project_read.rs                  # existing full-response path remains
  project_read_stream.rs           # project-read sink/generator path
  project_read_resources.rs        # may gain resource-result streaming helpers
  mod.rs                           # include project_read_stream

lp-fw/fw-esp32/src/
  transport.rs                     # route project read to streaming path when possible
  serial/io_task.rs                # write streamed response without full JSON Vec
```

The exact `lpc-wire/src/json/` module shape may need a small `json/mod.rs` depending on the current flat `json.rs` facade. Prefer a structure that preserves existing `lpc_wire::json::{to_string, from_str, from_slice}` imports.

## Architecture Summary

### Semantic JSON Writer

`lpc-wire` gets a small JSON event writer, not a full JSON library. It owns comma placement and JSON escaping so call sites do not write punctuation manually.

Intended shape:

```rust
let mut writer = JsonWriter::new(out);
let mut object = writer.object()?;
object.prop("revision")?.serde(&revision)?;
let mut results = object.prop("results")?.array()?;
results.item()?.serde(&ProjectReadResult::Nodes(nodes))?;
results.finish()?;
object.prop("probes")?.array()?.finish()?;
object.finish()?;
```

The writer should support:

- `object()` / `array()`.
- `prop(name)` and `item()` with automatic comma handling.
- Primitive values: string, bool, null, integer where useful.
- `serde(&T)` bridge for existing serializable objects.
- `base64_bytes(bytes)` or equivalent for binary fields.

The first implementation can use `ser_write_json` for the serde bridge on `no_std` paths and `serde_json` in host-only tests if easier. The writer itself should be `no_std + alloc` compatible where it lives in `lpc-wire`.

### Project Read Response Streaming

Add a streamed response writer that emits the existing JSON shape:

```json
{
  "revision": 123,
  "results": [ ... ],
  "probes": [ ... ]
}
```

The important difference is that `results` and `probes` are appended one at a time. On host, tests should prove the streamed JSON deserializes into the same `ProjectReadResponse` as the normal serde path.

### Engine Sink Path

The engine should keep:

```rust
Engine::read_project(request) -> ProjectReadResponse
```

for host/tests and straightforward clients.

Add a sink-oriented path, names to refine during implementation:

```rust
Engine::write_project_read(request, sink) -> Result<(), E>
```

or:

```rust
Engine::stream_project_read(request, &mut ProjectReadResponseWriter<W>) -> Result<(), E>
```

This path should:

- Write revision first.
- Iterate queries and produce one `ProjectReadResult` at a time.
- Iterate probes and produce one `ProjectProbeResult` at a time.
- Avoid collecting `Vec<ProjectReadResult>` / `Vec<ProjectProbeResult>` on ESP.

The first pass may still build each individual result as an owned struct. That scopes peak memory to one result/probe instead of the entire response. Later specializations can stream a single heavy result internally.

### Resource Payload Streaming

Runtime-buffer payloads are the first heavy special case.

The writer should be able to emit the payload JSON object while base64-encoding `bytes` directly into the stream. It should not allocate a second encoded `String` or encoded `Vec<u8>`.

This can be used inside the resource result streaming path for payloads. Summaries can remain normal serde initially.

### ESP Integration

ESP currently sends `WireServerMessage` through a channel to `io_task`, and `io_task` serializes the full message into `Vec<u8>` before writing chunks. This is the false streaming path that must be corrected for project reads.

The integration should be minimally invasive:

- Keep existing full-message channel/path for small messages such as heartbeat.
- Add a project-read streaming response path for request/response messages that are known to be large.
- Write `M!`, stream JSON, write `\n`.
- Preserve host-client parse compatibility.

If direct async writes from the semantic writer are awkward, use a small fixed scratch/chunk writer that flushes to serial. The crucial property is bounded peak memory, not zero buffering.

## Main Components And Interactions

- `JsonWrite`: minimal write trait used by `JsonWriter`; test adapters can collect chunks for assertions.
- `JsonWriter`: owns JSON punctuation, escaping, and value entry points.
- `JsonObject` / `JsonArray`: scoped helpers that track whether the next property/item needs a comma.
- `StreamingBase64`: emits base64 data directly into a JSON string value.
- `ProjectReadResponseWriter`: writes the response envelope and accepts result/probe items.
- `ProjectReadSink` or direct writer methods: bridge from engine query iteration to response output.
- ESP transport/serial writer: connects `ProjectReadResponseWriter` to USB serial chunks.

## Test Strategy

- Unit-test writer punctuation and escaping.
- Unit-test nested object/array construction.
- Unit-test serde bridge equivalence.
- Unit-test streamed `ProjectReadResponse` equivalence against normal serde JSON.
- Unit-test streaming base64 output against known base64 strings.
- Unit-test chunked writer behavior with very small chunk size to prove flushing happens incrementally.
- Add engine test for streaming project read equivalence to `Engine::read_project` on representative queries.
- Add or update ESP check/smoke validation where feasible.
