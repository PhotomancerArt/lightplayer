# Phase 02 — `Kind::AudioLevel` + per-Kind impl branches

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** nothing (starts the plan).
>
> **Parallel with:** Phase 01 (`binding.rs` on-disk shape). Disjoint
> files. **Sequencing note:** if Phase 01 lands first and changes the
> `Binding::Bus` constructor form, this phase's `default_bind` branch
> must use the new tuple form `Binding::Bus(ChannelName(...))`. If
> Phase 02 lands first, it uses the old struct form and Phase 01
> rewrites it. Either order works.

## Scope of phase

Add `Kind::AudioLevel` to `lp-domain/lp-domain/src/kind.rs` and wire
it through every per-Kind branch. This is the only new signal Kind in
M3; all other audio / beat / touch / motion Kinds defer until an
example demands them.

Reference: [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md)
§3 (Kind table, storage recipes), §8 (default binds), §11 (channel
naming).

**In scope:**

- `lp-domain/lp-domain/src/kind.rs`:
  - Add `AudioLevel` variant.
  - Add a `pub const AUDIO_LEVEL_BANDS: usize = 3;` constant near
    the existing `MAX_PALETTE_LEN` / `MAX_GRADIENT_STOPS`.
  - Add a `audio_level_struct()` helper (next to `color_struct`,
    `gradient_struct`, etc.).
  - Add the `Self::AudioLevel => audio_level_struct()` branch in
    `Kind::storage`.
  - Add `Self::AudioLevel => Dimension::Dimensionless` (or rely on
    the `_ =>` fallback if the existing match uses one — confirm
    by reading the file before patching).
  - Add `Self::AudioLevel => Free` to `Kind::default_constraint`.
  - Add `Self::AudioLevel => NumberInput` to
    `Kind::default_presentation` (placeholder — no level-meter
    widget Variant yet; lands additively).
  - Add `Self::AudioLevel => Some(Binding::Bus(ChannelName(...)))`
    (or `Binding::Bus { channel: ChannelName(...) }` if Phase 01
    hasn't landed yet) returning `audio/in/0/level` to
    `Kind::default_bind`.
- New tests inline in `kind.rs::tests` covering AudioLevel:
  storage shape (3-field struct of floats), default bind channel,
  default constraint = Free, default presentation = NumberInput.
- Update the `storage_is_exhaustive_and_concrete` test (or whatever
  it's called in the current file) to include `Kind::AudioLevel`
  in its iteration list.

**Out of scope:**

- Other audio Kinds (`AudioFft`, `Beat`, `Touch`, `Motion`,
  `AudioLoudness`, etc.) — defer.
- Changing the `Color` storage struct (still `{ space: i32, coords:
  vec3 }` per `kind.rs` today).
- Adding a level-meter `Presentation` variant — `NumberInput` is the
  M3 placeholder.
- TOML loading of `Kind::AudioLevel` defaults — that's Phase 05's
  job (the `{ low, mid, high }` table form).

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md) § "Code organization in Rust source files":

- Tests at the **bottom** of the file, never at the top.
- Inside `mod tests`: `#[test]` functions first, helpers below.
- Document the new variant in rustdoc with a one-paragraph
  description matching the style of the existing `Kind::Color` /
  `Kind::Gradient` rustdoc, plus a pointer to `quantity.md` §3.
- No "// Add AudioLevel here" type comments. Just the rustdoc.

## Sub-agent reminders

- Do **not** commit.
- Do **not** add other audio Kinds (`AudioFft`, etc.).
- Do **not** add a level-meter `Presentation` variant.
- Do **not** change existing `Kind` storage / dimension / etc.
  branches.
- Do **not** suppress warnings.
- If Phase 01 has not landed yet, use the existing `Binding::Bus {
  channel: ... }` struct-variant form. If Phase 01 has landed, use
  `Binding::Bus(...)` tuple form.
- If something blocks, stop and report back.
- Report back: list of changed files, validation output, any
  deviations.

## Implementation

### Constant (near `MAX_PALETTE_LEN` etc.)

```rust
/// Number of frequency bands carried by [`Kind::AudioLevel`]: low / mid /
/// high. See `docs/design/lightplayer/quantity.md` §3.
pub const AUDIO_LEVEL_BANDS: usize = 3;
```

### `Kind` variant

```rust
pub enum Kind {
    // … existing variants …

    /// Audio frequency-band levels (low / mid / high) as F32 RMS values.
    /// Default-binds to `audio/in/0/level` (`quantity.md` §8). Storage is a
    /// fixed `{ low: f32, mid: f32, high: f32 }` struct (no project-wide
    /// constant beyond [`AUDIO_LEVEL_BANDS`]). Default constraint is
    /// [`Constraint::Free`] — RMS may exceed 1.0 with boost; clamping is
    /// downstream policy.
    AudioLevel,
}
```

Place it after `Texture` in the enum order. Snake-case serialization
gives `"audio_level"` for the on-disk Kind name.

### Storage helper

```rust
fn audio_level_struct() -> LpsType {
    LpsType::Struct {
        name: Some(String::from("AudioLevel")),
        members: alloc::vec![
            StructMember { name: Some(String::from("low")),  ty: LpsType::Float },
            StructMember { name: Some(String::from("mid")),  ty: LpsType::Float },
            StructMember { name: Some(String::from("high")), ty: LpsType::Float },
        ],
    }
}
```

### `Kind::storage`

Add a branch:

```rust
Self::AudioLevel => audio_level_struct(),
```

### `Kind::dimension`

Either add `Self::AudioLevel => Dimension::Dimensionless` explicitly,
or rely on the `_ => Dimension::Dimensionless` fallback if present.
**Read the current file before patching** and pick whichever matches
the existing style. Add a comment if relying on the fallback would be
non-obvious.

### `Kind::default_constraint`

Add to the match arms (`Free` is the existing fallback in M2; if it
remains, no change is needed beyond confirming via test):

```rust
// Implicit via the existing `_ => Free` arm. If the file has been
// changed to enumerate every Kind explicitly, add:
Self::AudioLevel => Free,
```

### `Kind::default_presentation`

```rust
Self::AudioLevel => NumberInput,
```

(A future `LevelMeter` variant on `Presentation` lands additively.)

### `Kind::default_bind`

```rust
Self::AudioLevel => Some(Binding::Bus(ChannelName(String::from("audio/in/0/level")))),
// OR if Phase 01 hasn't landed:
// Self::AudioLevel => Some(Binding::Bus {
//     channel: ChannelName(String::from("audio/in/0/level")),
// }),
```

### Tests (at the bottom of `kind.rs::tests`)

```rust
#[test]
fn audio_level_storage_is_three_floats() {
    let s = Kind::AudioLevel.storage();
    match s {
        LpsType::Struct { members, .. } => {
            assert_eq!(members.len(), AUDIO_LEVEL_BANDS);
            assert_eq!(members[0].name.as_deref(), Some("low"));
            assert_eq!(members[1].name.as_deref(), Some("mid"));
            assert_eq!(members[2].name.as_deref(), Some("high"));
            for m in &members {
                assert_eq!(m.ty, LpsType::Float);
            }
        }
        _ => panic!("AudioLevel storage must be a Struct"),
    }
}

#[test]
fn audio_level_dimension_is_dimensionless() {
    assert_eq!(Kind::AudioLevel.dimension(), Dimension::Dimensionless);
}

#[test]
fn audio_level_default_constraint_is_free() {
    assert!(matches!(Kind::AudioLevel.default_constraint(), Constraint::Free));
}

#[test]
fn audio_level_default_presentation_is_number_input() {
    assert_eq!(Kind::AudioLevel.default_presentation(), Presentation::NumberInput);
}

#[test]
fn audio_level_default_bind_is_audio_in_level() {
    match Kind::AudioLevel.default_bind() {
        // Tuple form (Phase 01 landed):
        Some(Binding::Bus(ChannelName(s))) => assert_eq!(s, "audio/in/0/level"),
        // Struct form (Phase 01 not yet landed): match Bus { channel } and
        // assert channel.0.
        other => panic!("expected Bus(audio/in/0/level), got {other:?}"),
    }
}

#[test]
fn audio_level_serializes_as_snake_case() {
    let k = Kind::AudioLevel;
    let s = serde_json::to_string(&k).unwrap();
    assert_eq!(s, "\"audio_level\"");
}
```

### Update the exhaustive-storage test

The existing `storage_is_exhaustive_and_concrete` (or similar)
iterates a literal list of `Kind` variants. Add `Kind::AudioLevel` to
that list. If the test instead uses some macro-driven enumeration,
nothing to change.

## Validate

```bash
cargo check -p lp-domain
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**.

## Definition of done

- `Kind::AudioLevel` variant exists with its rustdoc.
- `AUDIO_LEVEL_BANDS = 3` constant exported from `kind.rs`.
- `audio_level_struct` helper at the bottom of `kind.rs`.
- `Kind::storage`, `Kind::dimension`, `Kind::default_constraint`,
  `Kind::default_presentation`, `Kind::default_bind` all handle
  `AudioLevel`.
- Six new tests in `kind.rs::tests` covering the above.
- The exhaustive-storage test (if present) includes `AudioLevel`.
- Snake-case serialization of `Kind::AudioLevel` is `"audio_level"`.
- No commit.

Report back with: list of changed files, validation output, whether
Phase 01 had landed by the time this phase ran (so the reviewer
knows which `Binding::Bus` form is used), and any deviations.
