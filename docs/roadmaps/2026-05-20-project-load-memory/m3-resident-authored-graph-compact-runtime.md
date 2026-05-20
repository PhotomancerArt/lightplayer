# M3: Resident Authored Graph To Compact Runtime Graph

## Goal

Avoid keeping full authored `NodeDef` values resident after nodes have been
resolved, attached, and converted into runtime state.

## Work

- Inventory all runtime uses of `loaded_node_def`, artifact `NodeDef` caches,
  and node-entry config cloning.
- Classify authored definition access as hot runtime, client/debug, or reload.
- Introduce a compact runtime node record with ids, typed params, bindings,
  status/state handles, and artifact references.
- Move full authored definitions into a cold path: flash reload, bounded cache,
  or diagnostic-only access.
- Ensure project stop/reload and client inspection continue to behave
  predictably.

## Deliverables

- A compact resident graph model.
- A documented compatibility path for APIs that still need authored project
  definitions.
- Before/after resident memory profiles for `basic` and `button-sign`.

## Validation

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

## Implementation Strategy

Full plan. This is the largest behavioral change because it affects what the
engine considers the canonical loaded project.
