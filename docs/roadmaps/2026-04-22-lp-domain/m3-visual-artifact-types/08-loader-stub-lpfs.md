# Phase 08 — `LpFs`-based artifact loader stub

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** Phase 07 merged. `cargo test -p lp-domain` passing.
>
> **Parallel with:** Phase 09 (examples corpus). Phase 09 produces
> the `.toml` files this phase loads in tests; the two can be drafted
> in parallel as long as Phase 09 lands before Phase 10's integration
> tests.

## Scope of phase

Stand up `lp-domain/lp-domain/src/artifact/load.rs` with a single
generic loader entry point that reads a TOML file via the
`lpfs::LpFs` trait, deserializes it into a typed Visual struct, and
materializes any `ValueSpec` defaults via `LoadCtx`.

Reference: [`docs/roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md`](../../roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md)
"LpFs-based loader stub". Loader file conventions follow
[`docs/design/lpfx/overview.md`](../../design/lpfx/overview.md).

**In scope:**

- `lp-domain/lp-domain/src/artifact/load.rs`:
  - `pub fn load_artifact<T, F>(fs: &F, path: &LpPath) -> Result<T,
    LoadError>` where `T: Artifact + DeserializeOwned`, `F:
    LpFs`.
  - `pub enum LoadError` with variants: `Io(FsError)`,
    `Utf8(core::str::Utf8Error)`, `Parse(toml::de::Error)`,
    `SchemaVersion { artifact_kind: &'static str, expected: u32,
    found: u32 }`.
  - Materialize `ValueSpec` defaults at load time (per `quantity.md`
    §7 / non-negotiable §6) by walking the loaded artifact's slots
    and calling `Slot::default_value(&mut LoadCtx)`. **Caching is
    out of scope**; this phase only verifies that materialization
    succeeds without panicking.
  - Validation: assert `loaded.schema_version == T::CURRENT_VERSION`;
    return `LoadError::SchemaVersion` otherwise.
- `lp-domain/lp-domain/src/artifact/mod.rs`:
  - `pub mod load;`
  - `pub use load::{load_artifact, LoadError};`
- `lp-domain/lp-domain/Cargo.toml`:
  - Add `lpfs = { workspace = true }` (or appropriate path dep).
  - Add the `std` feature gate if `LpFs` only compiles with std.
    Read the existing `lpfs` Cargo.toml first to confirm whether
    it builds in `no_std + alloc`.
- `lp-domain/lp-domain/src/lib.rs`:
  - `pub mod artifact;`
  - `pub use artifact::{load_artifact, LoadError};`

**Out of scope:**

- `LpFsMem` + `LpFsStd` selection logic; the loader is generic
  over `LpFs`.
- Caching loaded artifacts.
- Cross-artifact resolution (Stack referencing a missing Pattern).
- Migration framework (M5).
- Any `lp-cli` wiring.
- ProjectRoot / `project.json` discovery.
- Materializing `Slot::default_value` for **every** slot at load
  time (we do walk the tree, but the runtime cache is not built;
  `LoadCtx` is throwaway).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md):

- Tests at the **bottom** of `load.rs`, never at the top.
- `#[test]` first, helpers below in `mod tests`.
- Module-level rustdoc explains the loader's contract: which
  errors map to which `LoadError` variants, when materialization
  happens, what's *not* validated (cross-artifact resolution).
- Don't field-narrate `LoadError`; variant names + types carry it.

## Sub-agent reminders

- Do **not** commit.
- Do **not** add caching, migration, or cross-artifact resolution.
- Do **not** widen the loader API beyond
  `load_artifact<T>(fs, path)`.
- Do **not** parse `bindings` keys into `NodePropSpec` at load
  time. They stay raw strings (per Phase 07's TODO).
- Do **not** suppress warnings.
- The `std` feature gate question: if `lpfs` builds in `no_std +
  alloc`, no gating is needed. If it pulls `std`, `load_artifact`
  is gated behind `feature = "std"` on `lp-domain` and the
  feature pulls `lpfs/std`. Confirm before deciding.
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, whether
  `std` gating was needed, any deviations.

## Implementation

### `LoadError`

```rust
//! TOML artifact loader.
//!
//! Reads a `.toml` file via [`LpFs`], deserializes it into a typed
//! [`Artifact`] struct, validates `schema_version`, and walks the
//! loaded artifact to materialize its [`ValueSpec`] defaults at load
//! time (per `docs/design/lightplayer/quantity.md` §7 + non-
//! negotiable §6). Materialization happens through [`LoadCtx`]; the
//! resulting [`LpsValue`]s are not cached in M3 — this phase only
//! verifies materialization does not panic. The runtime cache lands
//! with binding resolution (M3+).
//!
//! Cross-artifact resolution (Stack pointing at a missing Pattern,
//! cycle detection on Visual references) is out of scope; this
//! loader handles a single file at a time.

use crate::error::Error as DomainError;
use crate::schema::Artifact;
use crate::value_spec::LoadCtx;
use alloc::string::String;
use lpfs::{error::FsError, LpFs};
use lp_model::path::LpPath;

/// Errors the loader can return.
#[derive(Debug)]
pub enum LoadError {
    /// Underlying [`LpFs::read_file`] failure.
    Io(FsError),
    /// File content was not valid UTF-8.
    Utf8(core::str::Utf8Error),
    /// TOML parse failure.
    Parse(toml::de::Error),
    /// `schema_version` did not match the artifact's `CURRENT_VERSION`.
    SchemaVersion {
        artifact_kind: &'static str,
        expected: u32,
        found: u32,
    },
    /// Domain-layer error during materialization.
    Domain(DomainError),
}

impl From<FsError> for LoadError                { fn from(e: FsError) -> Self { LoadError::Io(e) } }
impl From<core::str::Utf8Error> for LoadError   { fn from(e: core::str::Utf8Error) -> Self { LoadError::Utf8(e) } }
impl From<toml::de::Error> for LoadError        { fn from(e: toml::de::Error) -> Self { LoadError::Parse(e) } }
impl From<DomainError> for LoadError            { fn from(e: DomainError) -> Self { LoadError::Domain(e) } }
```

### `load_artifact`

```rust
/// Load a TOML artifact through [`LpFs`] and validate its
/// `schema_version` against `T::CURRENT_VERSION`. Materializes
/// embedded [`ValueSpec`] defaults via a throwaway [`LoadCtx`].
pub fn load_artifact<T, F>(fs: &F, path: &LpPath) -> Result<T, LoadError>
where
    T: Artifact + serde::de::DeserializeOwned,
    F: LpFs,
{
    let bytes = fs.read_file(path)?;
    let text = core::str::from_utf8(&bytes)?;
    let loaded: T = toml::from_str(text)?;

    // Schema version check.
    let found = artifact_schema_version(&loaded);
    if found != T::CURRENT_VERSION {
        return Err(LoadError::SchemaVersion {
            artifact_kind: T::KIND,
            expected: T::CURRENT_VERSION,
            found,
        });
    }

    // Walk the artifact's slots and materialize defaults; surface
    // any panics-as-errors. M3 throws the materialized values away.
    let mut ctx = LoadCtx::default();
    walk_and_materialize(&loaded, &mut ctx)?;

    Ok(loaded)
}

/// Read `schema_version` from a loaded artifact via a small
/// reflective trick: deserialize the file twice, second time through
/// a `{ schema_version: u32, .. }` struct. Or, more simply, require
/// `T` to expose a `fn schema_version(&self) -> u32` accessor —
/// adding it as a trait method on `Artifact` is the cleaner path.
fn artifact_schema_version<T: Artifact>(artifact: &T) -> u32 {
    artifact.schema_version()
}
```

> **Note: extending `Artifact` with `schema_version`.** Cleanest
> approach is to add `fn schema_version(&self) -> u32;` as a
> required method on the `Artifact` trait, and implement it for
> each Visual struct (one-liner returning `self.schema_version`).
> Sub-agent should make this change as part of Phase 08.
>
> An alternative that avoids touching `Artifact`: deserialize the
> file twice — first into a small `{ schema_version: u32 }`
> stub, then (after the version check passes) into `T`. This
> works but doubles the parsing cost. Prefer the trait
> extension.

### `walk_and_materialize`

For M3, "materialize" means: walk every `Slot` in the artifact
(including nested `ParamsTable`, `LiveCandidate.params`,
`PlaylistEntry.params`, etc. — but the inline-overrides params
are `BTreeMap<String, toml::Value>`, not Slots; skip those) and
call `slot.default_value(&mut ctx)` to confirm it doesn't panic.

```rust
fn walk_and_materialize<T: Artifact>(artifact: &T, ctx: &mut LoadCtx)
    -> Result<(), DomainError>
{
    // Each Visual's params are accessible through a small per-type
    // accessor: e.g. Pattern::params() -> &ParamsTable. The cleanest
    // way to do this without making the loader Visual-aware is to
    // add an `Artifact::walk_slots` trait method that yields a
    // `&Slot` iterator. Each Visual implements it.
    //
    // Implementation suggestion: add `fn walk_slots<F>(&self, mut f: F)
    // where F: FnMut(&Slot)` to Artifact, with default impl that
    // does nothing (in case a future artifact has no slots).
    artifact.walk_slots(|slot| {
        let _ = slot.default_value(ctx);
    });
    Ok(())
}
```

For the M3 Visuals:

- `Pattern`, `Effect`, `Transition`: walk `self.params.0` (the
  inner `Slot` of `ParamsTable`).
- `Stack`: walk `self.params.0`. Effects' params are inline
  `toml::Value` overrides — skip.
- `Live`, `Playlist`: nothing to walk yet (no top-level
  `[params]` block in M3).

Each Visual implements:

```rust
impl Artifact for Pattern {
    const KIND: &'static str = "pattern";
    const CURRENT_VERSION: u32 = 1;

    fn schema_version(&self) -> u32 { self.schema_version }

    fn walk_slots<F: FnMut(&Slot)>(&self, mut f: F) {
        f(&self.params.0);
    }
}
```

Or a default `walk_slots` body that does nothing, for Visuals
without top-level slots:

```rust
fn walk_slots<F: FnMut(&Slot)>(&self, _f: F) {}
```

> **Note on the `Artifact` trait extension.** Phase 08 adds two
> required methods to `Artifact`:
>
> - `fn schema_version(&self) -> u32;`
> - `fn walk_slots<F: FnMut(&Slot)>(&self, f: F);`
>   (with a no-op default body so future Artifact kinds without
>   Slots don't have to implement it)
>
> Update Phase 06 / Phase 07 Visual structs to implement both.
> Phase 08's PR is the one that lands them, since the trait
> extension is the loader's deliverable.

### `artifact/mod.rs`

```rust
//! Artifact-loading machinery.

pub mod load;

pub use load::{load_artifact, LoadError};
```

### `lib.rs`

Add to the existing crate-root re-exports:

```rust
pub mod artifact;

pub use artifact::{load_artifact, LoadError};
```

### Tests (at the bottom of `load.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual::Pattern;
    use lpfs::lp_fs_mem::LpFsMem;
    use lp_model::path::LpPathBuf;

    fn fs_with(file: &str, body: &str) -> LpFsMem {
        let fs = LpFsMem::new();
        fs.write_file(&LpPathBuf::from(file).as_lp_path(), body.as_bytes()).unwrap();
        fs
    }

    #[test]
    fn loads_minimal_pattern() {
        let fs = fs_with("/test.pattern.toml", r#"
            schema_version = 1
            title = "Tiny"
            [shader]
            glsl = "void main() {}"
        "#);
        let p: Pattern = load_artifact(&fs, &LpPathBuf::from("/test.pattern.toml").as_lp_path()).unwrap();
        assert_eq!(p.title, "Tiny");
    }

    #[test]
    fn missing_file_returns_io_error() {
        let fs = LpFsMem::new();
        let res: Result<Pattern, _> =
            load_artifact(&fs, &LpPathBuf::from("/missing.toml").as_lp_path());
        assert!(matches!(res, Err(LoadError::Io(_))));
    }

    #[test]
    fn invalid_toml_returns_parse_error() {
        let fs = fs_with("/bad.pattern.toml", "not = valid\nrandom = ");
        let res: Result<Pattern, _> =
            load_artifact(&fs, &LpPathBuf::from("/bad.pattern.toml").as_lp_path());
        assert!(matches!(res, Err(LoadError::Parse(_))));
    }

    #[test]
    fn schema_version_mismatch_is_caught() {
        let fs = fs_with("/test.pattern.toml", r#"
            schema_version = 999
            title = "Wrong version"
            [shader]
            glsl = "void main() {}"
        "#);
        let res: Result<Pattern, _> =
            load_artifact(&fs, &LpPathBuf::from("/test.pattern.toml").as_lp_path());
        match res {
            Err(LoadError::SchemaVersion { expected: 1, found: 999, .. }) => {}
            other => panic!("expected SchemaVersion mismatch, got {other:?}"),
        }
    }

    #[test]
    fn materializes_default_values_without_panic() {
        let fs = fs_with("/full.pattern.toml", r#"
            schema_version = 1
            title = "Full"
            [shader]
            glsl = "void main() {}"
            [params.speed]
            kind    = "amplitude"
            default = 0.5
            [params.tint]
            kind    = "color"
            default = { space = "oklch", coords = [0.7, 0.15, 90] }
        "#);
        // load_artifact would panic in walk_and_materialize if Slot::
        // default_value broke. A successful return is the assertion.
        let _: Pattern = load_artifact(&fs, &LpPathBuf::from("/full.pattern.toml").as_lp_path()).unwrap();
    }
}
```

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

If `lpfs` is std-only and the loader is gated:

```bash
cargo check -p lp-domain --features std
cargo test  -p lp-domain --features std
```

All must pass with **zero warnings**.

## Definition of done

- `load_artifact<T, F>(fs, path)` exists and round-trips at least
  one example through `LpFsMem`.
- `LoadError` enum has the four documented variants + `Domain`
  for downstream errors.
- `Artifact` trait extended with `schema_version()` and
  `walk_slots()`. All M3 Visuals implement both.
- Schema-version mismatch is caught and surfaced as
  `LoadError::SchemaVersion`.
- `walk_slots` materialization at load time runs for every
  Visual that has a `[params]` block; tests verify no panic.
- `lpfs` dep wired in `lp-domain/Cargo.toml`; std-feature gating
  applied iff needed.
- Module re-exports updated (`artifact/mod.rs`, `lib.rs`).
- New tests cover: minimal load, missing file, parse error,
  schema-version mismatch, materialization smoke test.
- All pre-existing tests still pass.
- No commit.

Report back with: list of changed files, validation output,
whether `std` gating was needed, the chosen `Artifact`
extension shape, and any deviations.
