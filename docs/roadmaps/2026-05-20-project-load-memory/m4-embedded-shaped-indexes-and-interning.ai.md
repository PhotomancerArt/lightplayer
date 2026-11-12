# M4: Embedded-Shaped Indexes And Interning

## Goal

Replace host-comfortable graph indexes with compact structures that match the
loaded-project access pattern.

## Work

- Audit `NodeTree`, `ArtifactStore`, `NodeBindingIndex`, slot maps, path maps,
  and channel/resource lookup maps.
- Identify indexes that are build-once after load and can become frozen arrays.
- Replace repeated strings and paths with interned ids where lookup frequency
  justifies it.
- Prefer sorted slices, small vectors, dense ids, and direct node-id addressing
  over `BTreeMap`/hash maps for small embedded sets.
- Keep mutation boundaries explicit for project reload or live-edit features.

## Deliverables

- A compact index design covering node path lookup, child lookup, artifact
  lookup, and binding lookup.
- Memory accounting for each replaced map or string family.
- Regression tests for graph lookup, binding resolution, and project load.

## Validation

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

## Implementation Strategy

Full plan. The changes are individually mechanical, but the index ownership
model should be settled up front to avoid several half-compatible maps living
side by side.
