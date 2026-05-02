# M4.1 summary: runtime buffer and detail sync projection

## What was built

- **Shared resource identity** in `lpc-model` (`ResourceRef`, `ResourceDomain`,
  `RuntimeBufferId`, `RenderProductId`) so engine, wire, and view share one
  vocabulary; stores stay in `lpc-engine`.
- **Node-owned resource init** via `NodeResourceInitContext` / `Node::init_resources`
  so core nodes allocate owned render products and runtime buffers (including
  fixture colors) instead of the loader pre-inserting placeholders.
- **`GetChanges` extended** with per-request resource summary specifiers (buffer /
  render product / all) and payload specifiers (`None`, `All`, `ByIds`) for
  buffers and render products. The server holds no subscription state; the
  client sends the full interest set each sync.
- **Engine projection** (`detail_projection`, `resource_projection`,
  `compatibility_projection`) builds legacy-shaped `node_details` with semantic
  compatibility wrappers for heavy fields (refs in-place) plus store summaries
  and materialized payloads when requested.
- **Wire** compatibility state types carry resource refs beside inline snapshots
  where needed; JSON round-trips and partial merge behavior are covered in tests.
- **`ProjectView` + resource cache** applies summaries and payloads, resolves
  buffer bytes and render-product texture bytes for helpers/UI, and supports dev
  auto-watch for selected details.
- **Tests** cover wire specifiers, client view resolution, engine scene/render
  and partial updates, and server tick with the new sync fields.

## Decisions for future reference

- **Single sync envelope:** All M4.1 sync stays under `GetChanges` (no separate
  resource RPCs for this milestone).
- **Refs in semantic fields:** Node details keep resource refs in named state
  fields (not a flat `node_resources` list) for compatibility with existing
  `ProjectView` merge paths; M4.5 may replace the long-term shape.
- **Per-request watches:** Interests are restated on every request; no
  server-side watch/session cleanup.
- **Id model:** Wire refs use `{ domain, id }`; ids are not reused for the
  lifetime of a loaded runtime (removal invalidates; recreate gets a new id).
- **Payload tiering:** Summaries are cheap and list-friendly; raw buffer and
  full native texture payloads are opt-in by specifier so serial/Wi-Fi clients
  can stay lean.
- **Manual validation:** `just demo` remains the user acceptance check for the
  temporary visual dev UI; agents should not treat it as automatable.

## Manual validation

Run `just demo` locally to confirm the desktop client shows node state and
resource-backed fields as expected. This phase did not run `just demo` in CI or
headless automation.
