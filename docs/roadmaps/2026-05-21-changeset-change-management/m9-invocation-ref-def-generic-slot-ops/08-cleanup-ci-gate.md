# Phase 08 — Cleanup + CI Gate

**Dispatch:** main | parallel: - | **Depends on:** 07

## Scope of phase

Final pass: formatting, clippy, full CI gate, plan `summary.md`.

**In scope:**

- `cargo +nightly fmt`
- `just check`
- `just test` or minimum `cargo test -p lpc-node-registry` + `fw-tests` emu tests
- Write `m9-invocation-ref-def-generic-slot-ops/summary.md`
- Update `docs/roadmaps/.../changeset-change-management/summary.md` — M9 entry

**Out of scope:** new features, engine M6 cutover.

## Cleanup checklist

- [ ] No `NodeDefRef` in `lp-core/` (except deprecated alias if explicitly kept — prefer none)
- [ ] No `def_slot` field
- [ ] No `project_node_def_mutation` / `CustomDef` in registry
- [ ] No `def = { path` in `examples/` or active tests
- [ ] `VariantSet` in `edit_op.rs` + serde test
- [ ] Warnings fixed

## Validate

```bash
rustup update nightly
just check
just test
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
```

Commit (when user asks) with message along:

`feat(lpc-model): NodeInvocation Ref|Def enum + VariantSet edit ops`

## summary.md template

- Status: complete
- Delivered: enum model, TOML wire, VariantSet, generic apply/diff
- Breaking: `def = { path }` → `ref =`
- Validation commands
