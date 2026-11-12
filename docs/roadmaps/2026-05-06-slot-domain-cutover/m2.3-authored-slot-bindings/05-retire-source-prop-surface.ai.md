# Phase 5: Retire Source Prop Surface

## Scope Of Phase

Delete or isolate obsolete `lpc-source/src/prop` code after the model-level
binding types are in place.

In scope:

- Audit all `lpc-source/src/prop` files.
- Delete files whose responsibilities moved to `lpc-model`.
- Remove `SrcBinding` if no longer needed.
- Remove or isolate `NodePropSpec` / prop-path-oriented source binding helpers
  if they only support the old binding model.
- Preserve `toml_color` or move it deliberately. It contains useful authored
  color parsing decisions and should not be swept away with obsolete prop code.
- Update exports and downstream imports.

Out of scope:

- Rewriting unrelated source artifact loading.
- Renaming every historical doc mention.
- Runtime node truth pass.

## Code Organization Reminders

- Do not add new code to `lpc-source/src/prop`.
- If a compatibility file must remain, document why and when it should be
  deleted.
- Keep exports explicit so old names do not remain available by accident.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-source/src/prop/mod.rs`
- `lp-core/lpc-source/src/prop/src_binding.rs`
- `lp-core/lpc-source/src/prop/src_shape.rs`
- `lp-core/lpc-source/src/prop/src_value_spec.rs`
- `lp-core/lpc-source/src/prop/kind_defaults.rs`
- `lp-core/lpc-source/src/lib.rs`
- any callers found by:

  ```bash
  rg -n "SrcBinding|SrcShape|SrcSlot|SrcValueSpec|kind_default|NodePropSpec" lp-core
  ```

Deletion guidance:

- Delete `SrcBinding` once callers use `BindingDef` / `BindingEndpoint`.
- Keep `toml_color` if color literal parsing still depends on it.
- Move literal value concepts to `lpc-model` only if they are durable model
  concepts; otherwise keep a small source-only adapter with a clear TODO.
- Remove stale tests tied to old `prop` semantics.
- Update README/docs only where they would mislead future work.

Expected outcome:

- `lpc-source/src/prop` is gone or dramatically smaller.
- New node defs do not import from `lpc-source::prop`.
- Search results for old prop vocabulary are either gone or clearly
  transitional.

## Validate

Run:

```bash
cargo fmt --package lpc-source --package lpc-model --package lpc-engine
cargo test -p lpc-source
cargo test -p lpc-model
cargo check -p lpc-engine
```
