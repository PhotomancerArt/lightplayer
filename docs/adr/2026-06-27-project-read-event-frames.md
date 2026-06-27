# ADR: Project Read Event Frames

- **Status:** Accepted
- **Date:** 2026-06-27
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

`ProjectReadResponse` could grow into a large JSON message containing shape
registries, node slot roots, resource summaries, product previews, and runtime
buffer payloads. Over browser serial and ESP32, those giant response frames
caused hard-to-debug transport failures and pushed firmware toward large
synchronous JSON chunk queues.

Increasing the queue or buffer size would only move the failure boundary while
spending scarce device RAM. Shape pagination helped one payload family, but it
left the protocol with multiple partial-read mechanisms and did not solve
runtime buffer or probe payloads.

## Decision

A `ProjectReadRequest` is answered by one or more same-request-id
`ProjectReadFrame` messages. Each frame carries ordered semantic
`ProjectReadEvent` values. A project read completes only when the client sees a
terminal `End` event, terminal project-read `Error` event, or top-level server
error.

Frames are batched to an encoded JSON budget of 8 KiB. The batcher never splits
JSON mid-event. If an event does not fit in an empty frame, that event must be
split into smaller semantic events rather than forcing transports to grow a
larger buffer.

`ProjectReadResponse` remains as an aggregate compatibility DTO for Studio and
tests. Clients reconstruct it with `ProjectReadCollector` until the UI consumes
events directly.

## Consequences

- Project reads are now an operation-specific streaming protocol, not a normal
  one-response request.
- Clients must keep the request pending across multiple same-id server
  messages.
- Server transports share one bounded frame sender instead of each transport
  inventing its own large-response workaround.
- ESP32 no longer needs the response-shaped project-read JSON chunk queue.
- Resource payloads can stream as bounded byte events while preserving native
  payload bytes for debugging.
- The compatibility aggregate remains useful, but it is no longer the wire
  response shape.

## Alternatives Considered

- **Larger ESP32 queue:** simple, but consumes RAM and leaves the same failure
  mode at a larger payload size.
- **Async giant JSON writer:** would reduce one firmware queue issue, but still
  requires transports such as WebSocket/TCP to buffer a large logical message.
- **Binary protocol:** likely useful later, but larger than this refactor and
  not required to establish the bounded-message contract.
- **Shape pagination:** removes one large payload source, but creates special
  pagination semantics and does not cover resources or probes.
- **Node `def` root elision:** worthwhile optimization, but it should not be
  required for correctness.

## Follow-ups

- Let Studio apply project-read events progressively instead of rebuilding a
  full aggregate response.
- Add binary payload encoding if JSON/base64 overhead becomes material.
- Add root-level partial slot sync once `SlotMirrorView` can apply partial root
  snapshots safely.
- Revisit node `def` root elision and other payload-reduction optimizations
  after the bounded transport contract has settled.
