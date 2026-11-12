# Future Work

## Runtime State Watch API

- **Idea:** Let clients request/watch node runtime state roots such as `state`, `state.output`, or
  `state.compile_error`.
- **Why not now:** M2.6 only establishes the state-root model.
- **Useful context:** This likely replaces the old "detail" toggle.

## Compile Error State

- **Idea:** Add `ShaderState.compile_error` as an optional string slot.
- **Why not now:** `output` is the minimal first slice.
- **Useful context:** Compile errors belong in public runtime state, not in the render product value.

## RuntimeProduct Cleanup

- **Idea:** Revisit whether `RuntimeProduct::Value(LpsValueF32)` should become `RuntimeProduct::Value(LpValue)`.
- **Why not now:** Shader ABI values still use `LpsValueF32` heavily.
- **Useful context:** `LpValue::RenderProduct` proves graph products can move into the model without forcing all engine values to move at once.
