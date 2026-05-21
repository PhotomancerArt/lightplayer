# Unlimited Shape Read Is Silently Paged

- **Severity:** P2
- **Status:** fixed
- **First seen:** 2026-05-21-review.md
- **Last reviewed:** 2026-05-21-review.md
- **Owner:** unassigned

## Finding

`Engine::read_project` no longer honors an unlimited `ShapeReadQuery`.
When `limit` is `None`, the non-streaming path returns only the first 64
static/dynamic shapes and sets `complete` from that page. That changes the
meaning of existing requests such as `ProjectReadRequest::default_debug(None)`,
whose shape query explicitly has `limit: None`.

The streaming writer still treats `limit: None` as a full shape response, so the
two read paths now have different semantics.

## Evidence

- `lp-core/lpc-engine/src/engine/project_read_shapes.rs:17` - the `limit:
  None` branch calls `snapshot_page_with_static_catalog(query.after, 64)`.
- `lp-core/lpc-wire/src/messages/project_read/project_read_request.rs:58` -
  the default debug shape query still sends `limit: None`.
- `lp-core/lpc-engine/src/engine/project_read_stream.rs:136` - the streaming
  `limit: None` branch writes the full registry snapshot and marks it complete.

## Impact

Any caller that uses `Engine::read_project` directly, or any future fallback
that relies on the semantic response path, can receive an incomplete registry
for an unlimited request. That is currently masked because the generated static
catalog is 37 shapes, but projects with enough dynamic/runtime shapes can cross
the hard-coded 64 item page. The client then receives roots/patches whose shapes
may not be present in the response it asked to be complete.

## Suggested fix

Pick one semantic and make both paths match:

- Preserve `limit: None` as a full response for host/non-streaming
  `Engine::read_project`, accepting that this path is not the embedded low-memory
  path; or
- Change the wire contract/default request so callers must opt into paging, and
  make default debug requests send an explicit page limit.

The first option is the smaller compatibility fix.

## Fix

Implemented the compatibility fix: the non-streaming `limit: None` path now
uses an unbounded static-catalog snapshot page, preserving complete-response
semantics while keeping explicit `limit` requests paged.

## Validation

- Added `default_debug_shape_read_is_complete_without_limit`, which registers
  more than 64 dynamic shapes, calls
  `Engine::read_project(ProjectReadRequest::default_debug(None))`, and asserts
  the shape result is complete and contains every registered shape.
- Keep the existing streaming/full-response comparison test green.

## History

- 2026-05-21: opened by Codex review.
- 2026-05-21: fixed by restoring complete non-streaming semantics for unlimited shape reads.
