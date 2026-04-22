# Phase 4 — Trait surface: `Node` + `Artifact` + `Migration`

> Read [`00-notes.md`](./00-notes.md) and
> [`00-design.md`](./00-design.md) before starting.
>
> **Depends on:** Phase 2 (`lp-domain` skeleton + identity types)
> must be complete and `cargo test -p lp-domain` must pass before
> this phase runs.
>
> **Parallel with:** Phase 3 (`Kind` + `Constraint`). The two
> phases touch disjoint files:
> - This phase: `node/mod.rs`, `schema/mod.rs`, `artifact/mod.rs`.
> - Phase 3: `kind.rs`, `constraint.rs` (and minimal stubs for
>   `presentation.rs` / `binding.rs`).
>
> Neither phase modifies `lib.rs` or `types.rs`.

## Scope of phase

Implement the trait surface that artifact / node code targets:

1. `node/mod.rs` — `Node` trait. Runtime nodes implement this to
   expose their identity and property bag.
2. `schema/mod.rs` — `Artifact` trait (KIND, CURRENT_VERSION),
   `Migration` trait (KIND, FROM, migrate), empty `Registry`
   struct. Trait shapes only — no concrete artifact impls.
3. `artifact/mod.rs` — placeholder + a small re-export module so
   M3 has a clean place to fill in TOML parse / load logic.

**In scope:**

- `lp-domain/lp-domain/src/node/mod.rs` — full `Node` trait.
- `lp-domain/lp-domain/src/schema/mod.rs` — `Artifact`,
  `Migration`, `Registry`.
- `lp-domain/lp-domain/src/artifact/mod.rs` — placeholder doc
  comment + re-exports of `crate::schema::{Artifact, Migration,
  Registry}` so callers have a single ergonomic import path.
- A small error type (`DomainError` or similar) used by the
  `Node` trait return values. Define it in
  `lp-domain/lp-domain/src/error.rs` (new file, also declared
  from `lib.rs` — note that lib.rs already declares all the
  module names it expects in phase 2, so this needs a one-line
  addition to lib.rs **only if `error` isn't already in the
  declared list**; phase 2 does NOT declare `error`, so you have
  to add `pub mod error;` to lib.rs in this phase).
- Tests inline in each file:
  - `Node` trait: a tiny test impl that verifies the trait
    object-safety + signature.
  - `Artifact` + `Migration`: a tiny test impl confirming the
    associated constants compile and serde round-trip a synthetic
    `(KIND, version)` tuple to confirm the contract.
  - `Registry`: empty struct constructor compiles.

**Out of scope:**

- Concrete artifact types (`Pattern`, `Stack`, `Live`,
  `Playlist`, `Setlist`, `Show`) — M3.
- Real migration framework — M5.
- TOML parsing — M3.
- `LpFs`-based artifact loader — M3.
- `BindingResolver` real impl — M3+.

## Code Organization Reminders

- Tests at the **top** of each module.
- Helper functions at the **bottom**.
- One concept per file.
- Keep traits short and explicit.
- All public types derive `serde::{Serialize, Deserialize}` and
  `#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]`
  where serializable.
- Mark anywhere you take a shortcut for "M3 fills this in" with a
  `TODO(M3):` comment.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** expand scope. Do not add concrete artifact types.
- Do **not** suppress warnings or `#[allow(...)]` problems away.
- Do **not** disable, skip, or weaken existing tests.
- If something blocks completion, stop and report back.
- Report back: list of files changed, validation output, any
  deviations.

## Implementation Details

### `lp-domain/lp-domain/src/error.rs` (new)

```rust
//! Cross-cutting domain error. Concrete error variants land as concrete artifact
//! types and binding-resolver implementations come online (M3+).

use alloc::string::String;
use core::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum DomainError {
    UnknownProperty(String),
    PropertyTypeMismatch { expected: String, actual: String },
    Other(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProperty(p) => write!(f, "unknown property: {p}"),
            Self::PropertyTypeMismatch { expected, actual } => {
                write!(f, "property type mismatch: expected {expected}, got {actual}")
            }
            Self::Other(s) => f.write_str(s),
        }
    }
}

impl core::error::Error for DomainError {}
```

Add `pub mod error;` and `pub use error::DomainError;` to
`lib.rs`. (The other module declarations are already in place
from phase 2; only `error` needs to be added.)

### `lp-domain/lp-domain/src/node/mod.rs`

```rust
//! Node trait: the runtime interface every concrete node implements.

use crate::error::DomainError;
use crate::types::{NodePath, PropPath, Uid};
use crate::LpsValue;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec;

    struct DummyNode {
        uid: Uid,
        path: NodePath,
        speed: f32,
    }

    impl Node for DummyNode {
        fn uid(&self) -> Uid { self.uid }
        fn path(&self) -> &NodePath { &self.path }

        fn get_property(&self, prop: &PropPath) -> Result<LpsValue, DomainError> {
            match prop.first() {
                Some(crate::types::prop_path::Segment::Field(name)) if name == "speed" => {
                    Ok(LpsValue::F32(self.speed))
                }
                _ => Err(DomainError::UnknownProperty(prop_path_to_string(prop))),
            }
        }

        fn set_property(&mut self, prop: &PropPath, value: LpsValue) -> Result<(), DomainError> {
            match prop.first() {
                Some(crate::types::prop_path::Segment::Field(name)) if name == "speed" => {
                    match value {
                        LpsValue::F32(v) => { self.speed = v; Ok(()) }
                        other => Err(DomainError::PropertyTypeMismatch {
                            expected: "F32".to_string(),
                            actual: alloc::format!("{other:?}"),
                        }),
                    }
                }
                _ => Err(DomainError::UnknownProperty(prop_path_to_string(prop))),
            }
        }
    }

    fn prop_path_to_string(p: &PropPath) -> String {
        let mut out = String::new();
        for (i, seg) in p.iter().enumerate() {
            if i > 0 { out.push('.'); }
            match seg {
                crate::types::prop_path::Segment::Field(n) => out.push_str(n),
                crate::types::prop_path::Segment::Index(i) => {
                    out.push_str(&alloc::format!("[{i}]"));
                }
            }
        }
        out
    }

    #[test]
    fn node_is_object_safe() {
        let node: alloc::boxed::Box<dyn Node> = alloc::boxed::Box::new(DummyNode {
            uid: Uid(1),
            path: NodePath::parse("/main.show").unwrap(),
            speed: 1.0,
        });
        assert_eq!(node.uid(), Uid(1));
        assert_eq!(node.path().to_string(), "/main.show");
    }

    #[test]
    fn dummy_node_round_trips_speed() {
        let mut node = DummyNode {
            uid: Uid(7),
            path: NodePath::parse("/main.show").unwrap(),
            speed: 1.0,
        };
        let prop = vec![crate::types::prop_path::Segment::Field("speed".into())];
        node.set_property(&prop, LpsValue::F32(3.5)).unwrap();
        assert_eq!(node.get_property(&prop).unwrap(), LpsValue::F32(3.5));
    }
}

pub trait Node {
    fn uid(&self) -> Uid;
    fn path(&self) -> &NodePath;

    fn get_property(&self, prop: &PropPath) -> Result<LpsValue, DomainError>;
    fn set_property(&mut self, prop: &PropPath, value: LpsValue) -> Result<(), DomainError>;
}
```

> **`PropPath` element naming:** phase 2 defines `PropPath = Vec<prop_path::Segment>`
> where `Segment` is a re-export of `lps_shared::path::LpsPathSeg`. The
> variants are `Field(String)` and `Index(u32)` — verify the exact
> variant names in `lp-shader/lps-shared/src/path.rs` before writing the
> tests above and adjust if needed.

### `lp-domain/lp-domain/src/schema/mod.rs`

```rust
//! Schema layer: Artifact + Migration trait shapes; empty Registry.

use core::marker::PhantomData;

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyArtifact;
    impl Artifact for DummyArtifact {
        const KIND: &'static str = "dummy";
        const CURRENT_VERSION: u32 = 1;
    }

    struct DummyMigration;
    impl Migration for DummyMigration {
        const KIND: &'static str = "dummy";
        const FROM: u32 = 0;
        fn migrate(value: &mut toml::Value) {
            // bump a version field if present
            if let toml::Value::Table(t) = value {
                t.insert("version".into(), toml::Value::Integer(1));
            }
        }
    }

    #[test]
    fn artifact_constants_are_accessible() {
        assert_eq!(DummyArtifact::KIND, "dummy");
        assert_eq!(DummyArtifact::CURRENT_VERSION, 1);
    }

    #[test]
    fn migration_constants_are_accessible() {
        assert_eq!(DummyMigration::KIND, "dummy");
        assert_eq!(DummyMigration::FROM, 0);
    }

    #[test]
    fn migration_runs_against_toml_value() {
        let mut value = toml::Value::Table(toml::value::Table::new());
        DummyMigration::migrate(&mut value);
        match value {
            toml::Value::Table(t) => assert_eq!(t.get("version").unwrap().as_integer(), Some(1)),
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn registry_is_constructible() {
        let _: Registry = Registry::new();
    }
}

pub trait Artifact {
    const KIND: &'static str;
    const CURRENT_VERSION: u32;
    // TODO(M5): add `: serde::de::DeserializeOwned` and `: schemars::JsonSchema` bounds
    //          when the migration framework + codegen tooling come online.
}

pub trait Migration {
    const KIND: &'static str;
    const FROM: u32;

    fn migrate(value: &mut toml::Value);
}

#[derive(Default)]
pub struct Registry {
    // TODO(M5): replace with the real registry shape (artifact factories + migration chains).
    _stub: PhantomData<()>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
}
```

> **Note on `toml` in no_std.** The `toml` crate's `Value` type
> requires `alloc` (which we have). Verify with
> `cargo check -p lp-domain --no-default-features` that this
> compiles in our default config (no `std` feature). If `toml`
> requires `std` for `Value`, the right move is to gate the
> `Migration` trait body behind the `std` feature in M2 — but
> `toml = { workspace = true }` should already be configured to
> work in alloc-only mode in this workspace. Confirm the build
> before declaring done; **stop and report** if it doesn't.

### `lp-domain/lp-domain/src/artifact/mod.rs`

```rust
//! Artifact ergonomics: re-exports of the trait shapes from `crate::schema`.
//!
//! Concrete artifact types (`Pattern`, `Stack`, `Live`, `Playlist`, `Setlist`,
//! `Show`) land in M3 alongside their TOML loaders.

pub use crate::schema::{Artifact, Migration, Registry};
```

That's the entire file in M2.

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features std
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**. Optional:
`cargo +nightly fmt` on the new files.

## Definition of done

- `error.rs`, `node/mod.rs`, `schema/mod.rs`, `artifact/mod.rs`
  all contain real content (not stubs).
- `lib.rs` declares `pub mod error;` and re-exports
  `DomainError`.
- `Node` trait is object-safe, with `uid()`, `path()`,
  `get_property`, `set_property`.
- `Artifact` and `Migration` trait shapes match the spec.
- `Registry::new()` compiles.
- All tests pass with no warnings.
- No concrete artifact types added.
- No commit.

Report back with: list of files changed, validation output, and
any deviations (especially: did `toml::Value` work in
no_std + alloc as expected?).
