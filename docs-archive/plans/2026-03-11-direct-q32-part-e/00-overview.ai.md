# Plan E: Cleanup & Transform Removal

Part of the direct-Q32 design (docs/designs/2026-03-11-direct-q32).
Depends on Plan D (pipeline wiring).

## Goal

Remove the now-unused Q32 transform infrastructure and clean up stale
references from Plans A–D. After this, the compiler has a single code
path for Q32: direct emission via `NumericMode::Q32(Q32Strategy)`.

## Scope

| Area | Changes |
|------|---------|
| `backend/transform/` | Delete entirely (27 files) |
| `backend/q32/` | New module — relocate `Q32Options`, `float_to_fixed16x16`, `FixedPointFormat` |
| `backend/module/gl_module.rs` | Remove `apply_transform`, `apply_transform_impl`, `transform_single_function`, transform imports |
| `backend/mod.rs` | Remove `pub mod transform`, add `pub mod q32` |
| `lib.rs` | Update `Q32Options` re-export path |
| Dead files | Delete `lp_lib_fns.rs` |
| Stale comments | Fix ~10 comments referencing transform |
| `numeric.rs` | Fix doc comment, fix `float_cc_to_int_cc` Ordered/Unordered |

## Phases

1. Relocate shared Q32 types to `backend/q32/`
2. Delete `backend/transform/` and remove all references
3. Remove transform methods from `gl_module.rs`
4. Delete dead file `lp_lib_fns.rs`
5. Fix stale comments
6. Fix `float_cc_to_int_cc` Ordered/Unordered bug
7. Tests + validation
