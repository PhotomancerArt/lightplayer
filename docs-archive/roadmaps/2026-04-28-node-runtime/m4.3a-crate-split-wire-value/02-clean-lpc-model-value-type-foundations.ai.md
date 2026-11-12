# Phase 2 — Clean `lpc-model` Value/Type Foundations

## Scope of phase

Move `lpc-model` toward the shared core model shape by adding public
model-side `WireValue` and `WireType`, replacing shader-facing projections in
shared modules, and removing `lps-shared` from identity/addressing/type
foundations.

The final `lpc-model` crate must not depend on `lps-shared`, but this may only
be fully removable after Phase 3 moves source files such as `value_spec.rs` out
of `lpc-model`. This phase should eliminate `lps-shared` from the shared
modules; Phase 3 finishes the Cargo dependency removal if source files still
block it.

Out of scope:

- Do not move source/on-disk types to `lpc-source` yet, except as required to
  remove direct `lps-shared` dependencies from `lpc-model`.
- Do not move messages/tree deltas to `lpc-wire` yet.
- Do not add runtime conversion to `LpsValue`/`LpsType`; that belongs in
  Phase 5.
- Do not rewrite all dependent crates yet; keep compatibility shims only when
  needed for this phase's validation.
- Do not commit.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by a larger import/dependency tangle, stop and report.
- Report back: files changed, validation run, validation result, and any
  deviations from this phase.

## Implementation details

Start from:

- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/src/value_spec.rs`
- `lp-core/lpc-model/src/prop/kind.rs`
- `lp-core/lpc-model/src/prop/constraint.rs`
- `lp-core/lpc-model/src/prop/prop_path.rs`
- `lp-core/lpc-model/src/node/node_prop_spec.rs`
- `lp-shader/lps-shared/src/path.rs`

### Add `WireValue`

Create `lp-core/lpc-model/src/prop/wire_value.rs` (or a `value/` module if
that better matches the existing organization) and export it from
`prop/mod.rs` and crate root.

`WireValue` should be the public version of the existing private
`LpsValueWire` shape from `value_spec.rs`:

```rust
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WireValue {
    I32(i32),
    U32(u32),
    F32(f32),
    Bool(bool),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[f32; 2]; 2]),
    Mat3x3([[f32; 3]; 3]),
    Mat4x4([[f32; 4]; 4]),
    Array(Vec<WireValue>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, WireValue)>,
    },
}
```

Do not add `Texture2D` or runtime handle variants in this phase. If a texture
wire reference is needed by compile errors, add a narrow `Texture(TextureRef)`
with a separate `texture_ref.rs` file and a stable id only; otherwise defer it.

### Add `WireType`

Create `lp-core/lpc-model/src/prop/wire_type.rs` and export it from
`prop/mod.rs` and crate root.

`WireType` should be the model-side storage/type projection previously handled
by `LpsType`. Include the variants currently needed by `Kind::storage()` and
`Shape`/slot storage descriptions:

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireType {
    I32,
    U32,
    F32,
    Bool,
    Vec2,
    Vec3,
    Vec4,
    IVec2,
    IVec3,
    IVec4,
    UVec2,
    UVec3,
    UVec4,
    BVec2,
    BVec3,
    BVec4,
    Mat2x2,
    Mat3x3,
    Mat4x4,
    Texture2D,
    Array(Box<WireType>, usize),
    Struct {
        name: Option<String>,
        fields: Vec<WireStructMember>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireStructMember {
    pub name: String,
    pub ty: WireType,
}
```

Adjust names if the current local style prefers `Type`/`StructMember`; avoid
colliding with `lps_shared::LpsType`.

### Update `Kind`

Change `Kind::storage()` to return `WireType` instead of `LpsType`.

Do not keep an `LpsType` helper in `lpc-model`. Runtime/compiler conversion
belongs in `lpc-engine` Phase 5.

### Replace path dependency

`lpc-model` must not depend on `lps-shared`. If `prop_path.rs` and
`node_prop_spec.rs` currently use `lps_shared::path::{LpsPathSeg, parse_path}`,
copy or reimplement the small path segment/parser logic locally in
`lpc-model/src/prop/prop_path.rs`.

Keep the public API shape:

- `PropPath` remains the path type used by the rest of the code.
- `Segment` (or equivalent) remains exported if call sites use it.
- Existing parsing/formatting tests should pass.

Do not add a dependency from `lps-shared` back to `lpc-model`; keep this a
local model implementation for now.

### Remove `lps-shared` from shared model modules

Update `lp-core/lpc-model/Cargo.toml` only if all remaining files compile
without `lps-shared` in this phase:

- Remove the `lps-shared` dependency.
- Remove `lps-shared/std` from the `std` feature.
- Remove `lps-shared/schemars` from `schema-gen`.

Update `lib.rs`:

- Remove `pub use lps_shared::LpsType;`
- Remove `pub use lps_shared::LpsValueF32 as LpsValue;`
- Remove texture runtime/storage re-exports.
- Add `pub use` for `WireValue` and `WireType`.

If source-only files such as `value_spec.rs` still require `LpsValue` before
Phase 3, leave the Cargo dependency in place temporarily and report that Phase
3 must remove it after moving those files. Do not reintroduce `lps-shared` into
shared modules such as `kind.rs`, `prop_path.rs`, or `node_prop_spec.rs`.

## Tests to preserve/add

- Preserve existing `Kind::storage()` behavior semantically by checking
  representative kinds map to expected `WireType` variants.
- Preserve `PropPath` parse/format tests after removing `lps-shared`.
- Add a `WireValue` serde round-trip test for scalar, vector, array, and struct
  variants.

## Validate

Run:

```bash
cargo test -p lpc-model
cargo check -p lpc-model --no-default-features
```

Also verify shared modules no longer reference `lps_shared` directly:

```bash
rg "lps_shared|LpsType|LpsValue" lp-core/lpc-model/src/prop lp-core/lpc-model/src/node lp-core/lpc-model/src/lib.rs
```

It is acceptable in this phase if `value_spec.rs` still matches; that file
moves in Phase 3.

If formatting changed, run:

```bash
cargo +nightly fmt
```
