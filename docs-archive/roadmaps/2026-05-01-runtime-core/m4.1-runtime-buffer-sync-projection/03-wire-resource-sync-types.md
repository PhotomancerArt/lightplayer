# Phase 3: Wire resource sync types

## Scope of phase

Extend wire request/response types so `GetChanges` can carry resource summary
and explicit payload sync.

In scope:

- Add resource summary specifiers to `WireProjectRequest::GetChanges`.
- Add buffer payload and render-product payload specifiers.
- Add response structs/enums for resource summaries and full/native payloads.
- Keep render-product watch language distinct from runtime-buffer watches.

Out of scope:

- Engine projection implementation.
- Client cache implementation.
- Compression, chunking, preview/LOD implementation.

## Code organization reminders

- Keep wire types in `lpc-wire`, not engine.
- Use shared ids/refs from `lpc-model`.
- Put serialization tests near the bottom of the relevant module.
- Prefer explicit names over generic blobs.

## Sub-agent reminders

- Do not commit.
- Do not change protocol scope beyond M4.1.
- Do not suppress warnings or weaken tests.
- If serde shape is ambiguous, stop and report with options.

## Implementation details

Read:

- `00-notes.md` Q10-Q21.
- `00-design.md` "GetChanges resource specifiers" and "Resource summaries and payloads".
- `lp-core/lpc-wire/src/project/api.rs`
- `lp-core/lpc-wire/src/legacy/project/api.rs`
- `lp-core/lpc-wire/src/lib.rs`

Add request-side specifier types with `None`, `All`, and `ByIds` where needed:

- summary domains: runtime buffers, render products, all;
- runtime-buffer payload ids;
- render-product payload ids.

Add response-side types:

- resource summary entries with ref, domain, kind/type, changed frame, metadata,
  size hints, and status/availability where available;
- runtime-buffer payload with ref, changed frame, metadata, bytes;
- render-product payload with ref, changed frame, width, height, format, bytes.

Use full/native payloads only. Leave room in render-product request types for
future options if this can be done without implementing LOD now.

Update `SerializableProjectResponse` serialization/deserialization to include
the new response fields.

Add JSON round-trip tests for:

- empty specifiers/empty resource updates;
- all-domain summaries;
- by-id buffer payload request/response;
- by-id render-product payload request/response.

## Validate

Run:

```bash
cargo test -p lpc-wire resource
cargo test -p lpc-wire project
```
