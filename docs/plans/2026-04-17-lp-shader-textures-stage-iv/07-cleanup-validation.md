# Phase 7 — Cleanup and final validation

## Scope

Workspace-wide build + test pass. Confirm no consumer downstream of
`TextureStorageFormat` or LPIR ops broke from the new variants /
ops. Tidy up any TODOs introduced during the milestone.

## Code organization reminders

- Match arms over `TextureStorageFormat` should not have `_ => …`
  catch-alls anywhere in the workspace; the compiler tells us where
  to update.
- Match arms over `LpirOp` either explicitly handle each new op or
  group them with sensible existing variants; no silent fallthroughs
  for memory ops.

## Implementation details

### Survey for stale `match` arms

Search for every `match` over `TextureStorageFormat` and confirm all
three variants are handled or deliberately rejected with a clear
error message:

```bash
rg -n "TextureStorageFormat" --type rust
```

Likely sites:
- `lp-shader/src/engine.rs` — `expected_return_type` (handled in phase 6)
- `lp-shader/src/px_shader.rs` — any per-format dispatch (none today)
- Any consumer crates that pattern-match on the format

For each `match`, choose one:

1. Add explicit arms for `Rgb16Unorm` / `R16Unorm` if the operation
   is well-defined.
2. Return `LpsError::Validation` (or equivalent) with a "format not
   yet supported by <feature>" message if the consumer hasn't been
   taught about the new formats.

### Survey for stale `LpirOp` matches

```bash
rg -n "LpirOp::Store " --type rust
rg -n "LpirOp::Load " --type rust
```

Each site is reviewed: groups that mean "any memory op" should
include the six new variants (`Store8`, `Store16`, `Load8U`,
`Load8S`, `Load16U`, `Load16S`). This includes:

- Pretty printers / debug formatters
- Const-folding (`lpir/src/const_fold.rs`) — narrow ops are not
  foldable, but verify the pass doesn't crash when it encounters one
- Inliner pass (in feature/inline branch — confirm post-rebase)

### Workspace-wide build + test

```bash
cargo check --workspace --all-features
cargo test  --workspace --all-features
```

Resolve any compilation errors from new exhaustive `match` arms.

### Update roadmap status

Edit `docs/roadmaps/2026-04-16-lp-shader-textures/m1.1-lpir-format-prereqs.md`:

- Mark milestone "Done"
- Note the actual scope landed (six narrow ops, both formats)
- Link back to this plan directory

### Confirm M2.0 prerequisites met

Re-read `docs/roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md`.
Cross off the prerequisites list:

- ✅ `Store16` available (also `Store8`, `Load{8,16}{U,S}`)
- ✅ `R16Unorm`, `Rgb16Unorm` formats defined
- ✅ `compile_px` validates returns for all three formats
- ⏳ Stable function IDs from `feature/inline` — out of scope here

## Validate

```bash
cargo check --workspace --all-features
cargo test  --workspace --all-features
cargo clippy --workspace --all-features --all-targets
```

All green; M1.1 ready to merge.
