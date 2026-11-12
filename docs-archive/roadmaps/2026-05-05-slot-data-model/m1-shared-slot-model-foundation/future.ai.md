## Rich Static/Dynamic Authoring APIs

- **Idea:** Build ergonomic APIs and examples for static Rust-authored shapes and
  dynamic artifact-authored shapes.
- **Why not now:** Milestone 2 owns this once the foundational types exist.
- **Useful context:** Prior art lives under
  `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data`.

## Derive Macro

- **Idea:** Add `lpc-model-derive` for slot shape/data implementations.
- **Why not now:** Milestone 5 owns derive after manual slices validate the
  model.
- **Useful context:** Prior art lives under
  `/Users/yona/dev/photomancer/lpmini2024/crates/lp-data-derive`.

## ModelValue Rename

- **Idea:** Rename `ModelValue` / `ModelType` to shorter names like `Value` /
  `ValueShape` or `LpValue` / `LpType`.
- **Why not now:** Roadmap decision says the rename can wait until the slot model
  is applied broadly enough to justify the churn.
- **Useful context:** M1 only adds `ResourceRef` to `ModelValue`.
