# Phase 6 — schemars verification + cargo-check matrix

> Read [`00-notes.md`](./00-notes.md) and
> [`00-design.md`](./00-design.md) before starting.
>
> **Depends on:** Phase 5 complete; `cargo test -p lp-domain
> --features schema-gen` passes.

## Scope of phase

Verify schemars derives work end-to-end on every public type in
`lp-domain`, with a focused recursive-type smoke test on `Slot`
and `Shape`. Run the cargo-check matrix that downstream phases
will rely on. Document the schemars fallback chain in `lib.rs`
so future-us knows what to do if a derive ever breaks.

**In scope:**

- New test file
  `lp-domain/lp-domain/src/schema_gen_smoke.rs` (or inline
  test module in `lib.rs`) with `schemars::schema_for!` calls
  on every public type, gated on `feature = "schema-gen"`.
- Recursive-type smoke test on `Slot` and `Shape` — assert the
  generated schema doesn't panic and isn't trivially empty.
- Build matrix: run all five cargo invocations from the spec
  below, fix any warnings.
- Documentation block in `lib.rs` describing the schemars
  fallback chain.

**Out of scope:**

- Generating and committing schema JSON files — M4.
- Writing `lp-cli schema generate` command — M4.
- Schema drift / version pinning — M4.

## Code Organization Reminders

- The smoke test goes in a new `#[cfg(feature = "schema-gen")]
  mod schema_gen_smoke;` (file: `src/schema_gen_smoke.rs`),
  declared from `lib.rs`. This keeps the noise out of the
  primary modules.
- Tests at the top of the file, helpers at the bottom.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** modify the public types themselves — if a derive
  fails, stop and report (don't paper it over). The fallback
  chain is documented but should not be triggered in M2.
- Do **not** suppress warnings or `#[allow(...)]` problems away.
- Do **not** disable tests.
- If something blocks completion, stop and report back.
- Report back: matrix command output, any types that needed
  fallback, anything that surprised you.

## Implementation Details

### `lp-domain/lp-domain/src/schema_gen_smoke.rs`

```rust
//! Compile-time smoke test that `schemars::schema_for!` succeeds on every
//! public type in `lp-domain`. Gated on `feature = "schema-gen"`.

#![cfg(feature = "schema-gen")]

#[cfg(test)]
mod tests {
    use crate::binding::Binding;
    use crate::constraint::Constraint;
    use crate::kind::{Colorspace, Dimension, InterpMethod, Kind, Unit};
    use crate::presentation::Presentation;
    use crate::shape::{Shape, Slot};
    use crate::types::{
        ArtifactSpec, ChannelName, Name, NodePath, NodePathSegment, NodePropSpec, Uid,
    };
    use crate::value_spec::{TextureSpec, ValueSpec};
    use crate::LpsType;

    macro_rules! assert_schema_compiles {
        ($t:ty) => {{
            let schema = schemars::schema_for!($t);
            let json = serde_json::to_string(&schema).unwrap();
            assert!(!json.is_empty(), "schema for {} was empty", stringify!($t));
        }};
    }

    #[test] fn schema_uid()             { assert_schema_compiles!(Uid); }
    #[test] fn schema_name()            { assert_schema_compiles!(Name); }
    #[test] fn schema_node_path()       { assert_schema_compiles!(NodePath); }
    #[test] fn schema_node_path_seg()   { assert_schema_compiles!(NodePathSegment); }
    #[test] fn schema_node_prop_spec()  { assert_schema_compiles!(NodePropSpec); }
    #[test] fn schema_artifact_spec()   { assert_schema_compiles!(ArtifactSpec); }
    #[test] fn schema_channel_name()    { assert_schema_compiles!(ChannelName); }

    #[test] fn schema_dimension()       { assert_schema_compiles!(Dimension); }
    #[test] fn schema_unit()            { assert_schema_compiles!(Unit); }
    #[test] fn schema_colorspace()      { assert_schema_compiles!(Colorspace); }
    #[test] fn schema_interp_method()   { assert_schema_compiles!(InterpMethod); }
    #[test] fn schema_kind()            { assert_schema_compiles!(Kind); }
    #[test] fn schema_constraint()      { assert_schema_compiles!(Constraint); }

    #[test] fn schema_presentation()    { assert_schema_compiles!(Presentation); }
    #[test] fn schema_binding()         { assert_schema_compiles!(Binding); }
    #[test] fn schema_value_spec()      { assert_schema_compiles!(ValueSpec); }
    #[test] fn schema_texture_spec()    { assert_schema_compiles!(TextureSpec); }

    #[test] fn schema_shape()           { assert_schema_compiles!(Shape); }
    #[test] fn schema_slot()            { assert_schema_compiles!(Slot); }
    #[test] fn schema_lps_type()        { assert_schema_compiles!(LpsType); }

    #[test]
    fn slot_schema_is_recursive_and_non_trivial() {
        let schema = schemars::schema_for!(Slot);
        let json = serde_json::to_string(&schema).unwrap();
        // Slot's serialization mentions "shape", "label", "bind", "present" —
        // pick one that's stable across schemars versions.
        assert!(json.contains("shape"), "Slot schema should mention `shape`: {json}");
        // Slot is recursive via Shape::Array { element: Box<Slot>, ... } and
        // Shape::Struct { fields: Vec<(Name, Slot)>, ... }. The schema must
        // therefore have at least two definitions in its definitions table
        // (Slot itself + Shape, at minimum).
        assert!(
            json.contains("Slot") && json.contains("Shape"),
            "recursive schema lost Shape/Slot definitions: {json}",
        );
    }

    #[test]
    fn shape_schema_includes_all_variants() {
        let schema = schemars::schema_for!(Shape);
        let json = serde_json::to_string(&schema).unwrap();
        for variant in ["scalar", "array", "struct"] {
            assert!(
                json.to_lowercase().contains(variant),
                "Shape schema missing variant `{variant}`: {json}",
            );
        }
    }
}
```

Add `#[cfg(feature = "schema-gen")] mod schema_gen_smoke;` to
`lib.rs` so the file is wired in.

### `lib.rs` documentation block

Add a doc comment near the top of `lib.rs` (after the
`#![no_std]` line and the `extern crate alloc;` line):

```rust
//! ## schemars fallback chain (when a derive misbehaves)
//!
//! Every public type in this crate derives `schemars::JsonSchema` behind
//! the `schema-gen` feature. If a future derive fails (recursive-type
//! cycle, generic that schemars can't introspect, lifetime issue):
//!
//! 1. **Manual derive impl.** Hand-write `impl JsonSchema for T` returning
//!    a `schemars::schema::SchemaObject` that mirrors the serde shape.
//! 2. **Hand-written schema.** Drop the derive and ship a `pub fn
//!    <type>_schema() -> RootSchema` constructor that the codegen tool
//!    calls explicitly.
//! 3. **Drop schemars for the type.** As a last resort: remove the derive
//!    and document the type as "not part of the on-disk surface" so M4's
//!    codegen tool can skip it. M2 should not need this fallback for any
//!    type — if you find yourself reaching for it, stop and report.
//!
//! The smoke tests in `schema_gen_smoke.rs` catch broken derives early.
```

### Build matrix

Run all of:

```bash
cargo check -p lps-shared
cargo check -p lps-shared --features schemars
cargo test  -p lps-shared
cargo test  -p lps-shared --features schemars
cargo check -p lp-domain
cargo check -p lp-domain --features std
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**. Fix any that appear (do not
`#[allow]`).

Optional but recommended: run the firmware compile gate per
`.cursorrules` to confirm `lp-domain`'s no_std build is consistent
with the broader embedded build:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

This is **only required if `lp-domain` ends up in the firmware
dependency tree**. In M2 it almost certainly doesn't (firmware
crates have no reason to depend on it yet). Run it anyway if it
takes <60s; skip if it's slow. **Do not add `lp-domain` to fw
deps** in this phase — that's M3.

## Definition of done

- `schema_gen_smoke.rs` exists and is wired into `lib.rs`.
- All public types in `lp-domain` (the list in the macro tests
  above) have a passing `schema_for!` smoke test.
- The recursive-type smoke test on `Slot` / `Shape` passes.
- The build matrix passes with no warnings.
- The schemars fallback chain doc block is in `lib.rs`.
- No commit.

Report back with: full matrix command output (or a clean-pass
summary), and any types that surprised you (e.g. needed manual
adjustments to the smoke test assertions).
