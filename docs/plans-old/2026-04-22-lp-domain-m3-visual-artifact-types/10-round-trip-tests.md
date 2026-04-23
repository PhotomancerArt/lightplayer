# Phase 10 — Round-trip + schema-drift integration tests

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** Phases 08 (loader) and 09 (corpus) merged.
> `cargo test -p lp-domain` passing.

## Scope of phase

Stand up `lp-domain/lp-domain/tests/round_trip.rs` — the
integration suite that proves the loader, the serializer, and the
generated JSON schema all agree for every example in
`lp-domain/lp-domain/examples/v1/`.

Two paired assertions per example file:

1. **Loader ↔ serializer.** Load → serialize → reload →
   structural equality. Catches `Deserialize` ↔ `Serialize`
   drift (the most common bug class for this layer).
2. **Loader ↔ schema.** The parsed `toml::Value` validates
   against `schema_for!(<ArtifactType>)`. Catches `Deserialize` ↔
   `JsonSchema` drift, which would otherwise only show up as
   bad editor completions.

Reference: [`docs/roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md`](../../roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md)
"Round-trip tests".

**In scope:**

- `lp-domain/lp-domain/tests/round_trip.rs` — single integration
  test file.
- A small JSON-schema validator dep (e.g. `jsonschema`) added to
  `[dev-dependencies]` in `lp-domain/Cargo.toml`. Use whatever
  the workspace already depends on if anything; otherwise pick
  a maintained crate.
- Test helper(s) that walk the corpus directory at compile time
  (`include_str!` or runtime `std::fs::read_to_string`); both
  are fine since this is std-only integration.
- A separate `cargo test -p lp-domain --test round_trip` line
  in `justfile` if the project's justfile lists per-test
  invocations (otherwise the default `cargo test` covers it).

**Out of scope:**

- Cross-artifact resolution (Stack pointing at a missing
  Pattern). The loader doesn't resolve references in M3.
- Editor-completion tests; the schema-drift assertion is the
  proxy.
- Performance benchmarks for loading.
- Schema-codegen tooling (M4).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md):

- Integration tests live at `lp-domain/lp-domain/tests/`, not
  inline. Each test file is std-feature gated since `LpFs` /
  `toml` / `jsonschema` need std.
- Tests at the **bottom** of `round_trip.rs` if the file has
  shared helpers; otherwise a flat list of `#[test]` functions
  is fine.
- Helpers (e.g. `corpus_files()`) live above the `#[test]`
  functions when there are 2-3 helpers; or in a `mod helpers`
  sub-module if there are more. Either is fine; pick whatever
  reads cleaner for ~6 short helpers.
- Test names follow `<artifact>_<assertion>`:
  `pattern_round_trips`, `pattern_validates_against_schema`,
  `effect_round_trips`, ...

## Sub-agent reminders

- Do **not** commit.
- Do **not** add cross-artifact resolution.
- Do **not** broaden the loader API to validate the corpus; if
  the existing `load_artifact` from Phase 08 isn't enough, raise
  the gap and stop.
- Do **not** silently mutate the corpus to make tests pass; if
  the corpus needs a fix, document it and report back. The
  example files are the contract.
- Tests must fail when given the **old** M2-vintage TOML grammar
  (sanity check that the loader is actually validating).
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, count
  of round-trip + schema-drift assertions, total cargo-test
  wall time, any deviations.

## Implementation

### Layout of `tests/round_trip.rs`

```rust
//! Integration tests: load → serialize → reload → structural-equal,
//! and load → validate-against-schema. Run against every file under
//! `lp-domain/lp-domain/examples/v1/`. See M3 design notes
//! (`docs/plans/2026-04-22-lp-domain-m3-visual-artifact-types/`).

use lp_domain::artifact::{load_artifact, LoadError};
use lp_domain::visual::{
    Effect, Live, ParamsTable, Pattern, Playlist, Stack, Transition,
};
use lpfs::lp_fs_std::LpFsStd;
use lp_model::path::LpPathBuf;
use std::path::PathBuf;

const EXAMPLES_ROOT: &str = "examples/v1";

fn fs() -> LpFsStd {
    let crate_root = env!("CARGO_MANIFEST_DIR");
    let root = PathBuf::from(crate_root).join(EXAMPLES_ROOT);
    LpFsStd::new(root).expect("examples/v1 must exist")
}

#[test]
fn pattern_rainbow_round_trips() {
    let p: Pattern = load_artifact(&fs(), &lp("/patterns/rainbow.pattern.toml")).unwrap();
    assert_round_trip(&p);
}

#[test]
fn pattern_fbm_round_trips() {
    let p: Pattern = load_artifact(&fs(), &lp("/patterns/fbm.pattern.toml")).unwrap();
    assert_round_trip(&p);
}

#[test]
fn pattern_fluid_round_trips() {
    let p: Pattern = load_artifact(&fs(), &lp("/patterns/fluid.pattern.toml")).unwrap();
    assert_round_trip(&p);
}

#[test]
fn effect_tint_round_trips() {
    let e: Effect = load_artifact(&fs(), &lp("/effects/tint.effect.toml")).unwrap();
    assert_round_trip(&e);
}

#[test]
fn effect_kaleidoscope_round_trips() {
    let e: Effect = load_artifact(&fs(), &lp("/effects/kaleidoscope.effect.toml")).unwrap();
    assert_round_trip(&e);
}

#[test]
fn transition_crossfade_round_trips() {
    let t: Transition = load_artifact(&fs(), &lp("/transitions/crossfade.transition.toml")).unwrap();
    assert_round_trip(&t);
}

#[test]
fn transition_wipe_round_trips() {
    let t: Transition = load_artifact(&fs(), &lp("/transitions/wipe.transition.toml")).unwrap();
    assert_round_trip(&t);
}

#[test]
fn stack_psychedelic_round_trips() {
    let s: Stack = load_artifact(&fs(), &lp("/stacks/psychedelic.stack.toml")).unwrap();
    assert_round_trip(&s);
}

#[test]
fn live_main_round_trips() {
    let l: Live = load_artifact(&fs(), &lp("/lives/main.live.toml")).unwrap();
    assert_round_trip(&l);
}

#[test]
fn playlist_setlist_round_trips() {
    let p: Playlist = load_artifact(&fs(), &lp("/playlists/setlist.playlist.toml")).unwrap();
    assert_round_trip(&p);
}

// ---------- schema-drift assertions ----------

#[cfg(feature = "schema-gen")]
#[test]
fn pattern_corpus_validates_against_schema() {
    assert_validates::<Pattern>("/patterns/rainbow.pattern.toml");
    assert_validates::<Pattern>("/patterns/fbm.pattern.toml");
    assert_validates::<Pattern>("/patterns/fluid.pattern.toml");
}

#[cfg(feature = "schema-gen")]
#[test]
fn effect_corpus_validates_against_schema() {
    assert_validates::<Effect>("/effects/tint.effect.toml");
    assert_validates::<Effect>("/effects/kaleidoscope.effect.toml");
}

#[cfg(feature = "schema-gen")]
#[test]
fn transition_corpus_validates_against_schema() {
    assert_validates::<Transition>("/transitions/crossfade.transition.toml");
    assert_validates::<Transition>("/transitions/wipe.transition.toml");
}

#[cfg(feature = "schema-gen")]
#[test]
fn stack_corpus_validates_against_schema() {
    assert_validates::<Stack>("/stacks/psychedelic.stack.toml");
}

#[cfg(feature = "schema-gen")]
#[test]
fn live_corpus_validates_against_schema() {
    assert_validates::<Live>("/lives/main.live.toml");
}

#[cfg(feature = "schema-gen")]
#[test]
fn playlist_corpus_validates_against_schema() {
    assert_validates::<Playlist>("/playlists/setlist.playlist.toml");
}

// ---------- guard rails ----------

#[test]
fn old_m2_grammar_fails_to_load() {
    // Sanity check: a deliberately M2-vintage Pattern (using
    // `type = "f32"` / `min` / `max`) must be rejected.
    let toml = r#"
        title = "Old"
        [shader]
        glsl = "void main() {}"
        [params.speed]
        type = "f32"
        min  = 0.0
        max  = 1.0
        default = 1.0
    "#;
    let p: Result<Pattern, _> = toml::from_str(toml);
    assert!(p.is_err(), "old M2 grammar must fail with deny_unknown_fields");
}

// ---------- helpers (at the bottom) ----------

fn lp(s: &str) -> lp_model::path::LpPathBuf {
    LpPathBuf::from(s)
}

/// Load → serialize → reload → structural-equal.
fn assert_round_trip<T>(v: &T)
where
    T: PartialEq + std::fmt::Debug + serde::Serialize + serde::de::DeserializeOwned,
{
    let s = toml::to_string(v).expect("serialize");
    let back: T = toml::from_str(&s).expect("re-deserialize");
    assert_eq!(v, &back, "round-trip changed shape:\n{s}");
}

/// Validate the file's parsed TOML against the schema-gen JSON
/// schema for the type. Catches Deserialize ↔ JsonSchema drift.
#[cfg(feature = "schema-gen")]
fn assert_validates<T>(rel_path: &str)
where
    T: schemars::JsonSchema,
{
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(EXAMPLES_ROOT)
        .join(rel_path.trim_start_matches('/'));
    let toml_str = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let value: toml::Value = toml::from_str(&toml_str)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let json: serde_json::Value = serde_json::to_value(value).expect("toml→json");

    let schema = schemars::schema_for!(T);
    let schema_json = serde_json::to_value(&schema).expect("schema→json");

    let validator = jsonschema::validator_for(&schema_json)
        .unwrap_or_else(|e| panic!("compile schema for {}: {e}", std::any::type_name::<T>()));

    if let Err(errors) = validator.validate(&json) {
        let mut msg = String::from("schema validation failed:\n");
        for e in errors {
            msg.push_str(&format!("  - {e}\n"));
        }
        panic!("{msg}");
    }
}
```

> **Note on the `jsonschema` crate.** Its API changes between
> majors. The exact `validator_for` / `validate` calls above
> match the v0.30+ shape; sub-agent should pin a specific
> version in `[dev-dependencies]` and adjust if the API has
> moved. If the workspace already depends on a JSON Schema
> validator, prefer that one for consistency.

> **Note on TOML→JSON conversion.** `toml::Value`
> `Serialize` produces JSON via `serde_json::to_value`
> faithfully (TOML's data types are a strict subset of JSON's).
> The one wart is TOML datetimes → JSON strings, which we
> don't use anywhere in M3.

### Cargo wiring

`lp-domain/lp-domain/Cargo.toml` `[dev-dependencies]`:

```toml
[dev-dependencies]
serde_json = { workspace = true }
jsonschema = "..."         # pick a current version
```

(Plus whatever was added in earlier phases; this is just the
new addition for Phase 10.)

If `lpfs` requires a `std` feature on `lp-domain`, this test
file is automatically gated — `LpFsStd` won't compile without
it. Document the gating in the file's `//!` header.

### Cross-cutting: `schema-gen` gating

The schema-validation tests are `#[cfg(feature = "schema-gen")]`.
The basic round-trip tests are not. Run both:

```bash
cargo test -p lp-domain --test round_trip
cargo test -p lp-domain --test round_trip --features schema-gen
```

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
cargo test  -p lp-domain --test round_trip
cargo test  -p lp-domain --test round_trip --features schema-gen
```

All must pass with **zero warnings**.

## Definition of done

- `tests/round_trip.rs` exists with at least 10 round-trip
  `#[test]` functions (one per example) and 6 schema-drift
  `#[test]` functions (one per Visual kind).
- Sanity-check test confirms M2-vintage grammar fails to load.
- `jsonschema` (or equivalent) added as dev-dep.
- Both feature configurations
  (`--features schema-gen` on/off) compile and pass.
- All pre-existing tests still pass.
- No commit.

Report back with: list of changed files, count of test functions
in `round_trip.rs`, total `cargo test` wall time before and
after, jsonschema crate version used, and any deviations.
