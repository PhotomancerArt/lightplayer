# Phase 1: Define CoreProjectRuntime And Service Surface

## Scope of Phase

Create the core project runtime module and a minimal runtime-services surface
that future phases can extend. This phase should compile without porting real
texture/shader/fixture/output behavior yet.

In scope:

- Add `lp-core/lpc-engine/src/project_runtime/`.
- Add `CoreProjectRuntime` as a small owner around `Engine` plus placeholders for
project services.
- Add a narrow `RuntimeServices` / service-handle API that can later expose
graphics, output sinks, render products, runtime buffers, and compatibility
projection.
- Add accessors needed by later phases.
- Export the module from `lpc-engine`.
- Add focused unit tests for runtime construction and accessors.

Out of scope:

- Real source project loading.
- Real node ports.
- Server wiring.
- Compatibility wire projection beyond a stub type.
- Deleting or modifying `LegacyProjectRuntime`.

## Code Organization Reminders

- Prefer granular files, one concept per file.
- Put public entry points first, support types next, helpers near the bottom.
- Tests live at the bottom of files.
- Keep temporary code rare. If a tactical shortcut is necessary, record it in
`docs/roadmaps/2026-05-01-runtime-core/m4-legacy-node-runtime-port/future.md`
with why it exists and why it was not cleaned up now.

## Sub-agent Reminders

- Do not commit.
- Stay strictly within this phase scope.
- Do not suppress warnings or add `#[allow(...)]` to hide problems.
- Do not disable, skip, or weaken tests.
- If blocked by a design issue, stop and report instead of improvising.
- Report changed files, validation results, and any deviations.

## Implementation Details

Read first:

- `docs/roadmaps/2026-05-01-runtime-core/m4-legacy-node-runtime-port/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m4-legacy-node-runtime-port/00-design.md`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/runtime_buffer/`
- `lp-core/lpc-engine/src/render_product/`
- `lp-core/lpc-engine/src/legacy_project/project_runtime/types.rs`
- `lp-core/lpc-engine/src/lib.rs`

Create:

- `lp-core/lpc-engine/src/project_runtime/mod.rs`
- `lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs`
- `lp-core/lpc-engine/src/project_runtime/runtime_services.rs`
- `lp-core/lpc-engine/src/project_runtime/compatibility_projection.rs`

Suggested initial API:

```rust
pub struct CoreProjectRuntime {
    engine: Engine,
    services: RuntimeServices,
    compatibility: CompatibilityProjection,
}

impl CoreProjectRuntime {
    pub fn new(root_path: TreePath, services: RuntimeServices) -> Self;
    pub fn engine(&self) -> &Engine;
    pub fn engine_mut(&mut self) -> &mut Engine;
    pub fn services(&self) -> &RuntimeServices;
    pub fn services_mut(&mut self) -> &mut RuntimeServices;
    pub fn compatibility(&self) -> &CompatibilityProjection;
    pub fn tick(&mut self, delta_ms: u32) -> Result<(), EngineError>;
}
```

`RuntimeServices` should be a real type, not an empty marker. Start with fields
that are safe to add now and can compile without threading graphics/output yet:

- optional project root path or descriptive root `TreePath`;
- service accessors for future store/service owners;
- a documented place for graphics/output handles to land in later phases.

Do not add fake behavior. If a service cannot be implemented yet, leave an API
out or add a documented placeholder type only if later phases need it to compile.

`CompatibilityProjection` can be a small struct with `new()` and no behavior yet,
with a module-level doc explaining M4 uses compatibility snapshots until M4.1.

Update `lp-core/lpc-engine/src/lib.rs` to export the new module and public types.

Tests:

- `CoreProjectRuntime::new` constructs an engine with the requested root path.
- `tick` delegates to `Engine::tick` for an empty runtime without panic.
- Services and compatibility accessors return stable references.

## Validate

Run:

```bash
cargo test -p lpc-engine project_runtime
```

