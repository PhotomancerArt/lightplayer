# Phase 1: Type-safe IDs and FrameTime

## Goal

Implement type-safe node IDs and frame timing structures that form the foundation of the runtime system.

## Tasks

1. Create `nodes/id.rs` with:
   - `TextureId(u32)`, `OutputId(u32)`, `ShaderId(u32)`, `FixtureId(u32)` newtype wrappers
   - `#[serde(transparent)]` attribute for each ID type (serializes as u32, becomes string in JSON)
   - `From<u32>` and `Into<u32>` implementations for each ID type
   - `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash` derives
   - Export from `nodes/mod.rs`

2. Create `runtime/frame_time.rs` with:
   - `FrameTime` struct with `delta_ms: u32` and `total_ms: u32` fields
   - `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq` derives
   - Export from `runtime/mod.rs` (create if needed)

3. Update `lib.rs` to export:
   - `pub mod runtime;` (if not already present)
   - Ensure `nodes` module is properly exported

4. Add tests:
   - Test ID serialization/deserialization (u32 ↔ string in JSON)
   - Test ID conversions (From/Into u32)
   - Test FrameTime struct creation and field access
   - Test FrameTime equality

## Success Criteria

- All ID types compile and serialize/deserialize correctly
- FrameTime struct compiles and works as expected
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style

