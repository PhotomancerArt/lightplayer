# Summary: Streaming Project Read JSON

## What Was Built

- Added a small semantic JSON writer in `lpc-wire` with object/array scopes, automatic comma handling, primitive writers, and a serde bridge.
- Added streaming base64 helpers and a manual runtime-buffer payload writer so binary payload fields do not allocate an encoded `String`.
- Added `ProjectReadResponseWriter` for result-by-result project-read envelope serialization.
- Added `Engine::write_project_read_json` as a sink-oriented project-read path with equivalence tests against `Engine::read_project`.
- Updated ESP serial output so project-read responses stream their server envelope, results, probes, and resource payload bytes instead of building one full serialized JSON `Vec`.

## Decisions For Future Reference

#### Keep JSON, Stream The Hot Path

- **Decision:** Keep the existing JSON document shape and add streaming writers for project reads.
- **Why:** This fixes the immediate peak-memory issue without forcing every client/server message to change at once.
- **Rejected alternatives:** Binary protocol now; replacing all serde message serialization now.
- **Revisit when:** JSON bandwidth or parsing cost becomes the limiting factor instead of peak allocation.

#### Specialize Resource Payload Bytes

- **Decision:** Runtime-buffer payload bytes use a manual base64 writer.
- **Why:** The existing serde helper allocates encoded text; payload bytes are the heavy field most likely to OOM on device.
- **Rejected alternatives:** Let serde handle payload bytes; delay resource payloads until a binary protocol.
- **Revisit when:** Resource reads gain explicit chunk/range support.

#### Server Tick Still Owns Semantic Responses

- **Decision:** Leave `lpa-server` returning owned `WireMessage`s in this plan.
- **Why:** Changing server tick into a response sink is a larger ownership/API pass. This plan removes the full serialized JSON buffer first.
- **Rejected alternatives:** Rebuild the server loop and transport trait immediately.
- **Revisit when:** Large slot snapshots or resource payload clones still dominate memory after streamed serial output.
