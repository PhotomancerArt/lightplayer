//! Integration tests: load → serialize → reload → structurally equal,
//! and (with `schema-gen`) load as TOML value → validate against
//! `schemars::schema_for!` JSON schema.
//!
//! Requires **`--features std`**: the artifact loader and `LpFsStd` are
//! std-only. Schema-drift subtests are additionally gated on
//! **`--features schema-gen`** (needs `schemars` and generated `JsonSchema`
//! for Visual types). Run:
//! - `cargo test -p lp-domain --features std --test round_trip`
//! - `cargo test -p lp-domain --features std,schema-gen --test round_trip`

use lp_domain::load_artifact;
#[cfg(feature = "schema-gen")]
use lp_domain::schema::Artifact;
use lp_domain::visual::{Effect, Live, Pattern, Playlist, Stack, Transition};
use lp_model::path::LpPathBuf;
use lpfs::lp_fs_std::LpFsStd;
use std::path::PathBuf;

const EXAMPLES_ROOT: &str = "examples/v1";

fn fs() -> LpFsStd {
    let crate_root = env!("CARGO_MANIFEST_DIR");
    let root = PathBuf::from(crate_root).join(EXAMPLES_ROOT);
    LpFsStd::new(root)
}

#[test]
fn pattern_rainbow_round_trips() {
    let p: Pattern = load_artifact(
        &fs(),
        LpPathBuf::from("/patterns/rainbow.pattern.toml").as_path(),
    )
    .unwrap();
    assert_round_trip(&p);
}

#[test]
fn pattern_fbm_round_trips() {
    let p: Pattern = load_artifact(
        &fs(),
        LpPathBuf::from("/patterns/fbm.pattern.toml").as_path(),
    )
    .unwrap();
    assert_round_trip(&p);
}

#[test]
fn pattern_fluid_round_trips() {
    let p: Pattern = load_artifact(
        &fs(),
        LpPathBuf::from("/patterns/fluid.pattern.toml").as_path(),
    )
    .unwrap();
    assert_round_trip(&p);
}

#[test]
fn effect_tint_round_trips() {
    let e: Effect = load_artifact(
        &fs(),
        LpPathBuf::from("/effects/tint.effect.toml").as_path(),
    )
    .unwrap();
    assert_round_trip(&e);
}

#[test]
fn effect_kaleidoscope_round_trips() {
    let e: Effect = load_artifact(
        &fs(),
        LpPathBuf::from("/effects/kaleidoscope.effect.toml").as_path(),
    )
    .unwrap();
    assert_round_trip(&e);
}

#[test]
fn transition_crossfade_round_trips() {
    let t: Transition = load_artifact(
        &fs(),
        LpPathBuf::from("/transitions/crossfade.transition.toml").as_path(),
    )
    .unwrap();
    assert_round_trip(&t);
}

#[test]
fn transition_wipe_round_trips() {
    let t: Transition = load_artifact(
        &fs(),
        LpPathBuf::from("/transitions/wipe.transition.toml").as_path(),
    )
    .unwrap();
    assert_round_trip(&t);
}

#[test]
fn stack_psychedelic_round_trips() {
    let s: Stack = load_artifact(
        &fs(),
        LpPathBuf::from("/stacks/psychedelic.stack.toml").as_path(),
    )
    .unwrap();
    assert_round_trip(&s);
}

#[test]
fn live_main_round_trips() {
    let l: Live = load_artifact(&fs(), LpPathBuf::from("/lives/main.live.toml").as_path()).unwrap();
    assert_round_trip(&l);
}

#[test]
fn playlist_setlist_round_trips() {
    let p: Playlist = load_artifact(
        &fs(),
        LpPathBuf::from("/playlists/setlist.playlist.toml").as_path(),
    )
    .unwrap();
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

/// Proves the hand-written `JsonSchema` for `Slot` is not permissive: malformed
/// JSON that cannot match a single `oneOf` arm must fail.
#[cfg(feature = "schema-gen")]
#[test]
fn slot_wire_json_schema_rejects_malformed_slot_tables() {
    use lp_domain::shape::Slot;

    let root = schemars::schema_for!(Slot);
    let schema_json = serde_json::to_value(&root).expect("schema→json");
    let v = jsonschema::validator_for(&schema_json).expect("compile slot schema");

    // Valid minimal free-`Constraint` scalar (sanity: schema is not "reject everything")
    let ok_free = serde_json::json!({
        "kind": "amplitude",
        "default": 0.0
    });
    assert!(v.is_valid(&ok_free), "expected minimal scalar to validate");

    // `kind` + `default` + array `shape` + `element`/`length` — not one wire shape.
    let mixed = serde_json::json!({
        "kind": "amplitude",
        "default": 0.0,
        "shape": "array",
        "length": 2,
        "element": { "kind": "amplitude", "default": 0.0 }
    });
    assert!(
        !v.is_valid(&mixed),
        "scalar keys mixed with array slot keys must be rejected"
    );

    // Range arm has `additionalProperties: false` — no stray keys.
    let unknown_key = serde_json::json!({
        "kind": "amplitude",
        "default": 0.0,
        "range": [0.0, 1.0],
        "not_a_legitimate_key": 1
    });
    assert!(
        !v.is_valid(&unknown_key),
        "unknown property on a range scalar must be rejected"
    );
}

// ---------- guard rails ----------

#[test]
fn old_m2_grammar_fails_to_load() {
    // M2 used `type` / `min` / `max` at the param table; M3 `ParamsTable` /
    // `Slot` + deny_unknown_fields must reject it.
    let toml = r#"
        schema_version = 1
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
    assert!(
        p.is_err(),
        "old M2 grammar must fail to deserialize (unknown fields / shape)"
    );
}

// ---------- helpers (at the bottom) ----------

/// Load → TOML serialize → re-deserialize → structural equality (Q15: not byte-on-disk).
fn assert_round_trip<T>(v: &T)
where
    T: PartialEq + std::fmt::Debug + serde::Serialize + serde::de::DeserializeOwned,
{
    let s = toml::to_string(v).expect("serialize");
    let back: T = toml::from_str(&s).expect("re-deserialize");
    assert_eq!(v, &back, "round-trip changed shape:\n{s}");
}

/// Validate JSON produced from the **loaded, typed** artifact against
/// `schemars::schema_for!(T)`. `JsonSchema` for these types matches serde’s JSON
/// shape; raw `toml::Value` → JSON does not (TOML-specific `Slot` / table
/// layouts), so we validate the same in-memory value the loader returns.
#[cfg(feature = "schema-gen")]
fn assert_validates<T>(rel_path: &str)
where
    T: schemars::JsonSchema + serde::Serialize + serde::de::DeserializeOwned + Artifact,
{
    let loaded: T = load_artifact(&fs(), LpPathBuf::from(rel_path).as_path())
        .unwrap_or_else(|e| panic!("load {rel_path}: {e:?}"));
    let json: serde_json::Value = serde_json::to_value(&loaded).expect("struct→json");

    let schema = schemars::schema_for!(T);
    let schema_json = serde_json::to_value(&schema).expect("schema→json");

    let validator = jsonschema::validator_for(&schema_json)
        .unwrap_or_else(|e| panic!("compile schema for {}: {e}", std::any::type_name::<T>()));

    if !validator.is_valid(&json) {
        let mut msg = String::from("schema validation failed:\n");
        for e in validator.iter_errors(&json) {
            msg.push_str(&format!("  - {e}\n"));
        }
        panic!("{msg}");
    }
}
