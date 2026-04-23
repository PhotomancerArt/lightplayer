# Phase 2 — `lp-domain` crate skeleton + identity & addressing types

> Read [`00-notes.md`](./00-notes.md) and
> [`00-design.md`](./00-design.md) before starting.
>
> **Depends on:** Phase 1 (`lps-shared` serde + schemars derives)
> must be complete and `cargo check -p lps-shared --features
> schemars` must pass before this phase runs.

## Scope of phase

Create the `lp-domain/lp-domain/` crate with `Cargo.toml` and
`src/lib.rs`, wire it into the workspace, and implement the
identity & addressing types in `src/types.rs`.

**In scope:**

- New directory `lp-domain/lp-domain/` with:
  - `Cargo.toml` (no_std + alloc default; `std` and `schema-gen`
    features; deps: `lps-shared`, `serde`, `schemars`,
    `toml`, `lpfs` (optional, behind `std`)).
  - `src/lib.rs` (no_std preamble, `extern crate alloc`,
    re-exports of lps-shared types as `LpsType`, `LpsValue`,
    `TextureStorageFormat`, `TextureBuffer`; declares all the
    submodules — even ones phase 3/4/5 will fill, declared as
    empty `pub mod foo;` stubs).
  - `src/types.rs` (this phase's substantive content).
  - **Empty stub files** for the other modules so phase 3/4/5
    can fill them without touching `lib.rs` again. Each stub:
    one-line `//!` doc comment + nothing else.
- Workspace `Cargo.toml` updates:
  - Add `"lp-domain/lp-domain"` to `[workspace] members`.
  - Add `"lp-domain/lp-domain"` to `[workspace] default-members`.
- Identity & addressing types in `src/types.rs`:
  - `Uid(pub u32)` newtype, `Copy + Clone + Eq + Hash + Ord +
    PartialEq + PartialOrd + Debug + Display`. Display prints
    decimal. Serde + cfg-attr schemars derives.
  - `Name(pub String)` newtype with a constructor
    `Name::parse(s: &str) -> Result<Name, NameError>` enforcing
    `[A-Za-z0-9_]+` and non-empty. Display = inner. Serde +
    cfg-attr schemars derives.
  - `NodePathSegment { name: Name, ty: Name }` — represents one
    segment of `<name>.<type>` form.
  - `NodePath(pub Vec<NodePathSegment>)` with
    `NodePath::parse(s: &str) -> Result<NodePath, PathError>`
    accepting `/`-separated segments, each `<name>.<type>`.
    Display joins with `/` and a leading `/` (e.g.
    `/main.show/fluid.vis`).
  - `PropPath` — re-export of
    `lps_shared::path::LpsPathSeg` and `parse_path` under a
    `prop_path` alias module: `pub mod prop_path { pub use
    lps_shared::path::{LpsPathSeg as Segment, PathParseError,
    parse_path}; }`. Top-level `pub type PropPath =
    alloc::vec::Vec<prop_path::Segment>;`. Note: the `Index`
    variant carries `usize`, not `u32`.
  - `NodePropSpec { node: NodePath, prop: PropPath }`
    with `NodePropSpec::parse(s: &str) -> Result<...>` handling
    `"<nodepath>#<proppath>"` form. Display joins with `#`.
  - `ArtifactSpec(pub String)` — opaque newtype, no parsing in
    v0 (M3 owns format). Serde + cfg-attr schemars derives.
    Display = inner.
  - `ChannelName(pub String)` — opaque newtype, convention-only
    (`<kind>/<dir>/<channel>[/<sub>...]` per `quantity.md` §11).
    No format enforcement. Serde + cfg-attr schemars derives.
    Display = inner.
- Tests inline at the top of `src/types.rs` (per `.cursorrules`):
  - `Uid` Display formats decimal; equality + hashing work.
  - `Name::parse` accepts `"foo"`, `"foo_bar_42"`, `"_x"`;
    rejects `""`, `"1foo"`, `"foo-bar"`, `"foo bar"`.
  - `NodePath::parse` accepts `"/main.show"`,
    `"/main.show/fluid.vis"`,
    `"/dome.rig/main.layout/sector4.fixture"`. Display
    round-trips.
  - `NodePath::parse` rejects empty, missing leading slash,
    missing dot in segment, double slash.
  - `PropPath` (via re-export) parses `"speed"` and
    `"config.spacing"` successfully.
  - `NodePropSpec::parse("/main.show/fluid.vis#speed")`
    round-trips through Display.
  - `NodePropSpec::parse` rejects missing `#`, double `#`.
  - `ArtifactSpec` and `ChannelName` Display round-trip.
- Empty stub files for: `kind.rs`, `constraint.rs`, `shape.rs`,
  `value_spec.rs`, `binding.rs`, `presentation.rs`,
  `node/mod.rs`, `schema/mod.rs`, `artifact/mod.rs`. Each just
  contains:

  ```rust
  //! TODO(M2 phase N): implement <module purpose>.
  ```

  Replace `<module purpose>` with a one-line summary (e.g.
  `Kind enum and per-Kind impls`). Replace `phase N` with the
  phase number that fills it (3 / 3 / 5 / 5 / 5 / 5 / 4 / 4 / 4).
  This lets `lib.rs` declare all modules now, and later phases
  drop content into the existing files without touching `lib.rs`.

**Out of scope:**

- `Kind`, `Constraint`, `Shape`, `Slot`, `ValueSpec`,
  `Binding`, `Presentation`, `Node`, `Artifact`, `Migration` —
  later phases.
- TOML grammar for any path type — file-relative parsing only,
  string-newtype convention.
- `lp-cli` integration of any kind.

## Code Organization Reminders

- One concept per file: `types.rs` only contains identity &
  addressing types. Don't sneak quantity-model types in here.
- Tests at the **top** of the module (per `.cursorrules`).
- Helper utility functions at the **bottom** of the file.
- No comments narrating what code does — only non-obvious intent.
- Mark stub files with `TODO(M2 phase N)` so they're greppable.

## Sub-agent Reminders

- Do **not** commit. The plan commits at the end as a single
  unit.
- Do **not** expand scope. Stay strictly within this phase.
- Do **not** implement `Kind`, `Shape`, `Slot`, etc. — even if
  the empty stub feels temptingly thin. Later phases own them.
- Do **not** suppress warnings or `#[allow(...)]` problems away
  — fix them.
- Do **not** disable, skip, or weaken existing tests to make the
  build pass.
- If something blocks completion (ambiguity, unexpected design
  issue), stop and report back rather than improvising.
- Report back: what changed, what was validated, and any
  deviations from the phase plan.

## Implementation Details

### 1. `lp-domain/lp-domain/Cargo.toml`

```toml
[package]
name = "lp-domain"
description = "LightPlayer domain model: identity, addressing, and the Quantity model"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = []
std = ["lpfs/std", "toml/std"]
schema-gen = ["std", "lps-shared/schemars", "dep:schemars"]

[dependencies]
lps-shared = { path = "../../lp-shader/lps-shared" }
lpfs = { path = "../../lp-base/lpfs", optional = true }
serde = { workspace = true, features = ["derive"] }
toml = { workspace = true }
schemars = { workspace = true, optional = true }

[dev-dependencies]
serde_json = { workspace = true }

[lints]
workspace = true
```

Note on the `schema-gen` feature: it's a *forward* hook for M4
(host-side codegen tooling). In M2 it doesn't gate anything by
itself; the `JsonSchema` derives below are always-on (compile in
no_std+alloc). The feature exists so the M4 sub-agent can flip
codegen behavior on without redefining what's already here.

If `lpfs` being `optional = true` triggers warnings about an
unused dependency (because nothing in M2 actually uses it), drop
the `optional = true` and the `std` feature gate above. M3 will
add the loader. **Decide based on what `cargo check` says.** If
the simpler option works, take it; if it warns, use the
`optional` form and feature-gate.

### 2. Workspace `Cargo.toml`

Find the `[workspace] members` block in the root `Cargo.toml`.
Add `"lp-domain/lp-domain"` in alphabetical-ish order (place it
near `lp-base/lpfs` since lp-domain conceptually sits next to
lp-base; the existing list is *not* strictly alphabetical so just
group it sensibly).

Find `default-members` and add `"lp-domain/lp-domain"` there too.

### 3. `lp-domain/lp-domain/src/lib.rs`

```rust
//! LightPlayer domain model.
//!
//! See [`docs/design/lightplayer/quantity.md`](../../../docs/design/lightplayer/quantity.md)
//! for the canonical Quantity-model spec this crate implements.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod artifact;
pub mod binding;
pub mod constraint;
pub mod kind;
pub mod node;
pub mod presentation;
pub mod schema;
pub mod shape;
pub mod types;
pub mod value_spec;

// Re-exports of lps-shared types that lp-domain layers semantic meaning on top of.
pub use lps_shared::{LpsType, TextureBuffer, TextureStorageFormat};
pub use lps_shared::LpsValueF32 as LpsValue;
```

### 4. `lp-domain/lp-domain/src/types.rs`

Code organization:

```rust
//! Identity and addressing types: Uid, Name, NodePath, PropPath, NodePropSpec, ArtifactSpec, ChannelName.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[cfg(test)]
mod tests {
    // tests at the top per .cursorrules
}

// --- Uid ----------------------------------------------------------------

#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Uid(pub u32);

impl fmt::Display for Uid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// --- Name ---------------------------------------------------------------

// (... Name definition + parse + tests ...)

// --- NodePath -----------------------------------------------------------

// --- PropPath -----------------------------------------------------------

pub mod prop_path {
    pub use lps_shared::path::{LpsPathSeg as Segment, PathParseError, parse_path};
}

pub type PropPath = Vec<prop_path::Segment>;

// --- NodePropSpec -------------------------------------------------------

// --- ArtifactSpec -------------------------------------------------------

// --- ChannelName --------------------------------------------------------

// --- Errors -------------------------------------------------------------
//   (NameError, PathError, NodePropSpecError) at the bottom

// --- Helper fns at the very bottom ---
```

Use `#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]`
on every public type. The `schema-gen` feature is what the codegen
tooling will toggle in M4; in M2 you can verify the derives compile
by running `cargo check -p lp-domain --features schema-gen`.

> **Important re: schema-gen feature wiring.** Because
> `schema-gen` activates `lps-shared/schemars` (which adds the
> `JsonSchema` derive on `LpsType`), the `lp-domain` types that
> reference `LpsType` (none in this phase, but several in phase 3
> and 5) only get a coherent `JsonSchema` impl when the feature
> is on. That's fine — schemars derives across the workspace are
> activated together via this feature.

#### `Name`

```rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct Name(pub String);

impl Name {
    pub fn parse(s: &str) -> Result<Self, NameError> {
        if s.is_empty() {
            return Err(NameError::Empty);
        }
        for c in s.chars() {
            if !(c.is_ascii_alphanumeric() || c == '_') {
                return Err(NameError::InvalidChar(c));
            }
        }
        if let Some(first) = s.chars().next() {
            if first.is_ascii_digit() {
                return Err(NameError::LeadingDigit);
            }
        }
        Ok(Name(String::from(s)))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
```

`NameError` is defined later in the file:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    LeadingDigit,
    InvalidChar(char),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("name is empty"),
            Self::LeadingDigit => f.write_str("name must not start with a digit"),
            Self::InvalidChar(c) => write!(f, "invalid character in name: {c:?}"),
        }
    }
}

impl core::error::Error for NameError {}
```

#### `NodePath`

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePathSegment {
    pub name: Name,
    pub ty: Name,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePath(pub Vec<NodePathSegment>);

impl NodePath {
    pub fn parse(s: &str) -> Result<Self, PathError> {
        let s = s.strip_prefix('/').ok_or(PathError::MissingLeadingSlash)?;
        if s.is_empty() {
            return Err(PathError::Empty);
        }
        let mut segments = Vec::new();
        for raw in s.split('/') {
            if raw.is_empty() {
                return Err(PathError::EmptySegment);
            }
            let (name, ty) = raw
                .split_once('.')
                .ok_or_else(|| PathError::SegmentMissingType(String::from(raw)))?;
            let name = Name::parse(name).map_err(PathError::InvalidName)?;
            let ty = Name::parse(ty).map_err(PathError::InvalidName)?;
            segments.push(NodePathSegment { name, ty });
        }
        Ok(NodePath(segments))
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for seg in &self.0 {
            write!(f, "/{}.{}", seg.name, seg.ty)?;
        }
        Ok(())
    }
}
```

Errors at the bottom:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    Empty,
    MissingLeadingSlash,
    EmptySegment,
    SegmentMissingType(String),
    InvalidName(NameError),
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("node path is empty"),
            Self::MissingLeadingSlash => f.write_str("node path must start with `/`"),
            Self::EmptySegment => f.write_str("node path has an empty segment"),
            Self::SegmentMissingType(s) => write!(f, "segment `{s}` is missing the `.<type>` suffix"),
            Self::InvalidName(e) => write!(f, "{e}"),
        }
    }
}

impl core::error::Error for PathError {}
```

#### `NodePropSpec`

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodePropSpec {
    pub node: NodePath,
    pub prop: PropPath,
}

impl NodePropSpec {
    pub fn parse(s: &str) -> Result<Self, NodePropSpecError> {
        let (node_part, prop_part) = s
            .split_once('#')
            .ok_or(NodePropSpecError::MissingHash)?;
        if prop_part.contains('#') {
            return Err(NodePropSpecError::ExtraHash);
        }
        let node = NodePath::parse(node_part).map_err(NodePropSpecError::Path)?;
        let prop = prop_path::parse_path(prop_part).map_err(NodePropSpecError::Prop)?;
        Ok(NodePropSpec { node, prop })
    }
}

impl fmt::Display for NodePropSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#", self.node)?;
        for (i, seg) in self.prop.iter().enumerate() {
            if i > 0 && matches!(seg, prop_path::Segment::Field(_)) {
                f.write_str(".")?;
            }
            match seg {
                prop_path::Segment::Field(name) => f.write_str(name)?,
                prop_path::Segment::Index(idx) => write!(f, "[{idx}]")?,
            }
        }
        Ok(())
    }
}
```

Note the leading-segment quirk: the first `Field` segment after
`#` should not have a leading `.`. Test that with
`"/x.y#a.b[0]"` → re-display equals input.

#### `ArtifactSpec` / `ChannelName`

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ArtifactSpec(pub String);

impl fmt::Display for ArtifactSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ChannelName(pub String);

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
```

### Tests

A representative set (extend as needed):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn uid_display_decimal() {
        assert_eq!(Uid(0).to_string(), "0");
        assert_eq!(Uid(7).to_string(), "7");
        assert_eq!(Uid(u32::MAX).to_string(), u32::MAX.to_string());
    }

    #[test]
    fn name_parse_accepts_valid() {
        for s in ["foo", "foo_bar_42", "_x", "X1"] {
            Name::parse(s).unwrap_or_else(|e| panic!("rejected {s:?}: {e}"));
        }
    }

    #[test]
    fn name_parse_rejects_invalid() {
        for s in ["", "1foo", "foo-bar", "foo bar", "foo.bar"] {
            assert!(Name::parse(s).is_err(), "should have rejected {s:?}");
        }
    }

    #[test]
    fn node_path_round_trips() {
        for s in [
            "/main.show",
            "/main.show/fluid.vis",
            "/dome.rig/main.layout/sector4.fixture",
        ] {
            let parsed = NodePath::parse(s).unwrap();
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn node_path_rejects_malformed() {
        for s in ["", "main.show", "/", "//", "/main", "/main.show//x.y"] {
            assert!(NodePath::parse(s).is_err(), "should have rejected {s:?}");
        }
    }

    #[test]
    fn prop_path_via_reexport() {
        let segs = prop_path::parse_path("config.spacing").unwrap();
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn node_prop_spec_round_trips() {
        let s = "/main.show/fluid.vis#speed";
        let parsed = NodePropSpec::parse(s).unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn node_prop_spec_with_indexing_round_trips() {
        let s = "/x.y#a.b[0]";
        let parsed = NodePropSpec::parse(s).unwrap();
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn node_prop_spec_rejects_missing_hash() {
        assert!(NodePropSpec::parse("/main.show").is_err());
    }

    #[test]
    fn node_prop_spec_rejects_double_hash() {
        assert!(NodePropSpec::parse("/main.show#a#b").is_err());
    }

    #[test]
    fn artifact_spec_display_round_trips() {
        assert_eq!(
            ArtifactSpec(String::from("./fluid.vis")).to_string(),
            "./fluid.vis",
        );
    }

    #[test]
    fn channel_name_display_round_trips() {
        assert_eq!(
            ChannelName(String::from("audio/in/0")).to_string(),
            "audio/in/0",
        );
    }
}
```

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features std
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**.

Optional: `cargo +nightly fmt` on the new files.

## Definition of done

- `lp-domain/lp-domain/Cargo.toml`, `src/lib.rs`, `src/types.rs`,
  and the eight stub files exist.
- Workspace `Cargo.toml` has `lp-domain/lp-domain` in both
  `members` and `default-members`.
- All five validation commands above pass with no warnings.
- Identity-type tests cover Uid, Name, NodePath, PropPath,
  NodePropSpec, ArtifactSpec, ChannelName.
- No `Kind`, `Constraint`, `Shape`, `Slot`, or any quantity-model
  code added.
- No commit.

Report back with: list of changed files, validation command output
(or a clean-pass summary), and whether the `lpfs` dep stayed
optional or not.
