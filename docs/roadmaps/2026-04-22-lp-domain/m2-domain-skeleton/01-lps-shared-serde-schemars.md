# Phase 1 — `lps-shared` serde + optional schemars derives

> Read [`00-notes.md`](./00-notes.md) and
> [`00-design.md`](./00-design.md) before starting.

## Scope of phase

Add `serde::{Serialize, Deserialize}` (always-on) and
`schemars::JsonSchema` (behind a new `schemars` feature) to
`lps_shared::types::LpsType` and `lps_shared::types::StructMember`.
Wire the workspace `schemars` dependency. Verify both
configurations compile and that the existing `lps-shared` tests
still pass.

**In scope:**

- `lp-shader/lps-shared/Cargo.toml` — add `serde` (always),
  `schemars` (optional). Add `schemars` to the `[features]`
  list. Keep the existing `default = []` and `std` feature.
- `lp-shader/lps-shared/src/types.rs` — add
  `#[derive(serde::Serialize, serde::Deserialize)]` and
  `#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]`
  to `LpsType` and `StructMember`. Adjust the existing
  `#[derive(Clone, Debug, PartialEq, Eq, Hash)]` line to add the
  serde derives.
- `lp-shader/lps-shared/src/path.rs` — add the same serde +
  cfg-attr schemars derives to `LpsPathSeg` (so `lp-domain`'s
  `NodePropSpec` and any future `PropPath`-carrying type can
  derive serde without hand-written impls). `PathParseError`
  does **not** need serde — it's a runtime error, not a
  serialized value.
- Workspace `Cargo.toml` — add `schemars = { version = "0.8",
  default-features = false }` to `[workspace.dependencies]`
  (alongside `serde` and `toml`). Use `serde` workspace dep with
  the `derive` feature so the `lps-shared` crate can request it.
  - **Verify the schemars version** by running
    `cargo search schemars --limit 1` once before pinning. Use
    `0.8` if the latest is in the 0.8.x line; otherwise use
    whatever is current and document the choice in the phase
    report. Keep `default-features = false` so the crate stays
    no_std-friendly. (`schemars` 0.8.x supports no_std-with-alloc
    by disabling default features.)
- Tests inline in `lp-shader/lps-shared/src/types.rs`:
  - serde JSON round-trip for a representative `LpsType` set
    (`Float`, `Vec3`, `Array { Float, 4 }`, a `Struct` with two
    members). Use `serde_json::to_string` then
    `serde_json::from_str`. Add `serde_json` as a `[dev-
    dependencies]` entry in `lp-shader/lps-shared/Cargo.toml`
    (workspace dep already exists).
  - `#[cfg(feature = "schemars")]` smoke test calling
    `schemars::schema_for!(LpsType)` and asserting the returned
    `RootSchema` is non-empty (just check the `definitions` map
    or the `schema.metadata` exists — don't over-specify).

**Out of scope:**

- Adding serde to `LpsValueF32` (we deliberately skip this;
  defaults flow through `ValueSpec` in lp-domain).
- Adding serde or schemars to any other type in `lps-shared`
  (e.g. `TextureStorageFormat`, `LayoutRules`) unless required
  for the derives above to compile.
- Touching `lp-domain` (doesn't exist yet — phase 2 creates it).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
  in each file.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be
  found later.
- Never narrate what code does in comments — only non-obvious
  intent / constraints / trade-offs.

## Sub-agent Reminders

- Do **not** commit. The plan commits at the end as a single
  unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away
  — fix them.
- Do **not** disable, skip, or weaken existing tests to make the
  build pass.
- If something blocks completion (ambiguity, unexpected design
  issue), stop and report back rather than improvising.
- Report back: what changed, what was validated, and any
  deviations from the phase plan.

## Implementation Details

### 1. Workspace deps

In root `Cargo.toml`, under `[workspace.dependencies]`, add (just
beneath the existing `toml = ...` line):

```toml
schemars = { version = "0.8", default-features = false }
```

(Confirm exact version with `cargo search schemars --limit 1`
before committing. If 1.x is current, use 1.x and note in the
phase report.)

### 2. `lps-shared/Cargo.toml`

Currently:

```toml
[package]
name = "lps-shared"
# ... unchanged ...

[features]
default = []
std = []

[dependencies]
lps-q32 = { path = "../lps-q32" }
```

Update to:

```toml
[package]
name = "lps-shared"
# ... unchanged ...

[features]
default = []
std = []
schemars = ["dep:schemars"]

[dependencies]
lps-q32 = { path = "../lps-q32" }
serde = { workspace = true, features = ["derive"] }
schemars = { workspace = true, optional = true }

[dev-dependencies]
serde_json = { workspace = true }
```

Note: `serde_json` is already a workspace dep (root `Cargo.toml`
line: `serde_json = { version = "1.0.80", default-features =
false, features = ["alloc"] }`).

### 3. `lps-shared/src/types.rs`

Replace the two existing derive lines:

Before (line ~5):

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LpsType {
```

After:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LpsType {
```

Before (line ~40):

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructMember {
```

After:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct StructMember {
```

Leave the other types in this file (`LayoutRules`) alone — they
aren't needed for the lp-domain surface this milestone.

### 3b. `lps-shared/src/path.rs`

Find the existing `LpsPathSeg` definition (~line 7):

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LpsPathSeg {
    Field(String),
    Index(usize),
}
```

Add serde + cfg-attr schemars:

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum LpsPathSeg {
    Field(String),
    Index(usize),
}
```

Leave `PathParseError` alone (no serde needed for an error
type). Existing tests in this file should continue to pass
unchanged.

### 4. Tests

Add a `#[cfg(test)] mod tests` block at the **top** of
`lps-shared/src/types.rs` (per `.cursorrules`: tests at the top of
the module). Existing tests already in the file (if any) can stay
where they are; just add the new ones in the same block or a
sibling block named `mod serde_tests`.

```rust
#[cfg(test)]
mod serde_tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;

    #[test]
    fn lps_type_scalar_roundtrip() {
        let original = LpsType::Float;
        let json = serde_json::to_string(&original).unwrap();
        let decoded: LpsType = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn lps_type_array_roundtrip() {
        let original = LpsType::Array {
            element: Box::new(LpsType::Float),
            len: 4,
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: LpsType = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn lps_type_struct_roundtrip() {
        let original = LpsType::Struct {
            name: Some(String::from("Color")),
            members: vec![
                StructMember {
                    name: Some(String::from("space")),
                    ty: LpsType::Int,
                },
                StructMember {
                    name: Some(String::from("coords")),
                    ty: LpsType::Vec3,
                },
            ],
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: LpsType = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[cfg(feature = "schemars")]
    #[test]
    fn lps_type_schema_for_succeeds() {
        let schema = schemars::schema_for!(LpsType);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(!json.is_empty());
        // Sanity: the root schema mentions the type name somewhere.
        assert!(json.contains("LpsType"));
    }
}
```

If the `serde_json` import fails because `lps-shared` is `no_std`,
note that `serde_json` workspace dep already opts into `alloc`
features, so it works in `no_std + alloc`. If for any reason it
does not, switch to `serde::ser::Serializer`/`Deserializer`-based
manual round-trip via `postcard` or write the test as a plain
`Serialize` invocation against a `Vec<u8>` sink. **Stop and ask
before adding a new dependency.**

## Validate

Run all of:

```bash
cargo check -p lps-shared
cargo check -p lps-shared --features schemars
cargo test  -p lps-shared
cargo test  -p lps-shared --features schemars
```

All must pass with **zero warnings**. If a warning appears, fix
it (do not `#[allow]`).

Optional but recommended: `cargo +nightly fmt --check` on the
edited files (the workspace pins nightly via
`rust-toolchain.toml`; `.cursorrules` requires `cargo +nightly
fmt` before commit).

## Definition of done

- `lps-shared` compiles with and without `--features schemars`.
- All four validation commands above pass with no warnings.
- `LpsType`, `StructMember`, and `LpsPathSeg` round-trip through
  serde JSON. Add a small round-trip test to `path.rs` mirroring
  the `types.rs` tests (one each for `Field("foo")` and
  `Index(3)`).
- `schemars::schema_for!(LpsType)` produces a non-empty schema
  under the feature.
- No other `lps-shared` files modified.
- No `lp-domain` directory created (phase 2's job).

Report back with: list of changed files, validation command output
(or a clean-pass summary), and the schemars version chosen.
