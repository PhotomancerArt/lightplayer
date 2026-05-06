# Phase 4: Mockup Generated Bootstrap

## Scope Of Phase

Apply build-generated static slot shape bootstrap to `lpc-slot-mockup`.

In scope:

- Add `lpc-slot-mockup/build.rs`.
- Add `lpc-slot-codegen` as a build-dependency.
- Include generated `OUT_DIR/slot_shapes.rs` from `lpc-slot-mockup/src/lib.rs`.
- Replace manual static shape registration lists with generated bootstrap.
- Keep dynamic shader-node shape registration explicit and manual.
- Add tests that prove generated registration covers mockup static roots.

Out of scope:

- Real `lpc-source` adoption.
- Changing mockup source data shape.
- Replacing dynamic shape registration.

## Code Organization Reminders

- Keep generated code included through one small public module.
- Keep dynamic shape registration close to the runtime owner.
- Keep tests readable and evidence-oriented.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-mockup/Cargo.toml`
- `lp-core/lpc-slot-mockup/build.rs`
- `lp-core/lpc-slot-mockup/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/model/mod.rs`
- `lp-core/lpc-slot-mockup/src/source/mod.rs`
- `lp-core/lpc-slot-mockup/src/engine/mod.rs`
- `lp-core/lpc-slot-mockup/src/engine/runtime.rs`

Expected changes:

- Build script calls `lpc_slot_codegen::generate_slot_shapes(...)`.
- `src/lib.rs` exposes:

```rust
pub mod slot_shapes {
    include!(concat!(env!("OUT_DIR"), "/slot_shapes.rs"));
}
```

- `model::register_shapes` should call generated
  `slot_shapes::register_all_static_slot_shapes`.
- Delete or simplify `source::register_shapes` and `engine::register_shapes`
  manual lists.
- Keep this in `MockRuntime::new()`:

```rust
registry.register_tree(dynamic_shape_id, shader_node.shape())?;
```

but add a short comment explaining that this is dynamic and instance/artifact
owned.

Tests:

- A mockup test should create an empty registry, call generated
  `register_all_static_slot_shapes`, and assert all static source/engine root
  ids are present.
- A mockup test should call generated `ensure_static_slot_shape` twice for the
  same id and assert idempotence.
- Existing mockup sync/tree/mutation/evidence tests should continue passing.

## Validate

```bash
cargo fmt --package lpc-slot-codegen --package lpc-slot-mockup
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup -- --nocapture
cargo clippy -p lpc-slot-mockup --all-targets -- -D warnings
```
