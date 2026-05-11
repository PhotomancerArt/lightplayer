### What was built

- Authored `/project.json` + `/src/*.kind` projects now load into a core
  `Engine` through `CoreProjectLoader`.
- `CoreProjectRuntime` owns the active server/demo runtime path, including
  runtime services, compatibility projection, engine ticking, and post-tick
  output flushing.
- First-class core MVP nodes now cover texture metadata, shader/pattern render
  products, fixture demand roots, and output sinks.
- Scene render/update and partial state tests exercise the M4 core path and its
  accepted metadata-only compatibility behavior.

### Decisions for future reference

#### Core runtime cutover before cleanup

- **Decision:** M4 switched `lpa-server` loading/ticking to
  `CoreProjectRuntime`; M5 owns old-runtime retirement.
- **Why:** The new runtime needed to prove the MVP shader -> fixture -> output
  flow before broad deletion work.
- **Rejected alternatives:** Keeping legacy and core runtimes as long-lived
  peers or adding an adapter layer around legacy runtime APIs.
- **Revisit when:** M5 removes or quarantines `LegacyProjectRuntime`.

#### Render products own visual output

- **Decision:** Shader/pattern output is exposed as a render product, not as an
  authoritative runtime buffer.
- **Why:** Visual output needs an opaque samplable product API that can later be
  backed by CPU, GPU, or embedded texture storage.
- **Rejected alternatives:** Treating texture pixels as ordinary runtime buffer
  bytes throughout the core node API.
- **Revisit when:** M4.1 defines sync refs and when render-product storage grows
  beyond the CPU copy scaffold.

#### Fixtures drive output

- **Decision:** Fixtures are demand roots; outputs are pushed sinks flushed
  after engine tick.
- **Why:** Fixtures own mapping and sampling decisions, while outputs only need
  dirty channel data and provider handles.
- **Rejected alternatives:** Making outputs pull data as demand roots or making
  output nodes own fixture mapping.
- **Revisit when:** Output sink teardown and many-to-many fixture/output mapping
  are designed.

#### Compatibility stays narrow

- **Decision:** M4 keeps metadata-only legacy wire compatibility and leaves
  buffer/render-product detail sync to M4.1.
- **Why:** The milestone needed server/demo continuity without locking in a
  frame-heavy client sync contract.
- **Rejected alternatives:** Rebuilding full legacy heavy `node_details`
  snapshots on top of the new runtime.
- **Revisit when:** M4.1 adds explicit runtime buffer/render-product refs,
  versions, cache behavior, and removal semantics.
