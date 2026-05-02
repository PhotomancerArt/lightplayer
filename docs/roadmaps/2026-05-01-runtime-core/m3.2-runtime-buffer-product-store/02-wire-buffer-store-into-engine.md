# Phase 2: Wire Buffer Store Into Engine

## Scope of Phase

Add `RuntimeBufferStore` ownership to `Engine` beside `RenderProductStore`.

In scope:

- Add a `runtime_buffers: RuntimeBufferStore` field to `Engine`.
- Initialize it in `Engine::new`.
- Add immutable and mutable accessors.
- Add focused engine tests.

Out of scope:

- Creating or consuming buffers from actual runtime nodes.
- `RuntimeProduct::Buffer`; that is Phase 3.
- Wire protocol changes.
- Any legacy runtime porting.

## Code Organization Reminders

- Keep the `Engine` public/accessor layout consistent with existing fields.
- Place tests at the bottom of the file in the existing test module.
- Keep helpers near the bottom of the test module.
- Any temporary code should have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by a missing Phase 1 API, stop and report.
- Report back: files changed, validation run, result, and any deviations.

## Implementation Details

Read first:

- `docs/roadmaps/2026-05-01-runtime-core/m3.2-runtime-buffer-product-store/00-design.md`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/runtime_buffer/mod.rs`

Update:

- `lp-core/lpc-engine/src/engine/engine.rs`

Add import:

```rust
use crate::runtime_buffer::RuntimeBufferStore;
```

Add field near `render_products`:

```rust
runtime_buffers: RuntimeBufferStore,
```

Initialize in `Engine::new`:

```rust
runtime_buffers: RuntimeBufferStore::new(),
```

Add accessors near `render_products()`:

```rust
pub fn runtime_buffers(&self) -> &RuntimeBufferStore {
    &self.runtime_buffers
}

pub fn runtime_buffers_mut(&mut self) -> &mut RuntimeBufferStore {
    &mut self.runtime_buffers
}
```

Tests:

- Add an engine test showing a buffer can be inserted through
  `runtime_buffers_mut()` and read through `runtime_buffers()`.
- Use a small `RuntimeBuffer::raw(...)` or texture helper from Phase 1 and a
  `Versioned` frame.
- Keep the test focused on ownership/accessors; do not build node adapters.

## Validate

Run:

```bash
cargo test -p lpc-engine engine
```
