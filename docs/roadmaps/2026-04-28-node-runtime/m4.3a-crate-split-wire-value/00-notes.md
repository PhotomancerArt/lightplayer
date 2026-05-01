# M4.3a — Crate split + `WireValue`: notes

# Scope of work

Decouple `lp-core/lpc-model` into focused crates, and introduce a
**`WireValue`** type so the wire / disk layers do not transitively
depend on `lp-shader::LpsValue` (and through it, the GLSL/JIT stack).

The end state, target shape:

```
lpc-model      (foundation primitives, no lps-shared dep)
  ├──► lpc-source     (on-disk authored source model)
  ├──► lpc-wire       (engine↔client wire shapes)
  ├──► lpc-engine     (engine runtime — boundary owner: LpsValueF32 → WireValue)
  └──► lp-view (client-side engine view/cache)
```

`lp-view` should not transitively depend on `lp-shader`. That
is the success criterion.

[`./plan.md`](./plan.md) is the short index into this folder.

# Current state of the codebase

## What `lpc-model` contains today (post-M4.2)

`lpc-model/src/lib.rs` already declares a tentative split via two
comment-banners:

- *Foundation (Quantity model + artifact traits)* —
  `artifact`, `prop`, `node`, `presentation`, `schema`, `types`, `value_spec`,
  `error`.
- *Protocol / project surface* —
  `bus`, `json`, `lp_config`, `lp_path`, `message`, `project`,
  `serde_base64`, `serial`, `server`, `state`, `transport_error`, `tree`.

That split isn't quite right (artifact concerns are mixed into
"foundation"; `bus` is foundation-ish but tagged "protocol"; `tree`
mixes `tree_path` (foundation) with `tree_delta` (protocol)) — but
it shows the conceptual boundary already exists in someone's head.

## File-level audit (proposed audience)

| File / module                          | Today                | Proposed crate    |
| -------------------------------------- | -------------------- | ----------------- |
| `node/node_id.rs`                      | foundation           | `lpc-model`       |
| `node/node_name.rs`                    | foundation           | `lpc-model`       |
| `node/node_spec.rs`                    | foundation           | `lpc-model`       |
| `node/node_prop_spec.rs`               | foundation           | `lpc-model`       |
| `node/node_config.rs`                  | misplaced            | `lpc-source`      |
| `node/node_props.rs` (legacy trait)    | misplaced            | `lpl-model` *(retires in M5)* |
| `prop/prop_path.rs`                    | foundation           | `lpc-model`       |
| `prop/prop_value.rs`                   | foundation           | `lpc-model`       |
| `prop/prop_namespace.rs`               | foundation           | `lpc-model`       |
| `prop/kind.rs`                         | foundation (uses `LpsType` for `storage()`) | **see Q-A** |
| `prop/constraint.rs`                   | foundation (uses `lps-shared`) | **see Q-A** |
| `prop/binding.rs`                      | misplaced            | `lpc-source`      |
| `prop/shape.rs` (Slot, Shape)          | misplaced            | `lpc-source`      |
| `prop/prop_access.rs`                  | misplaced (sync trait) | `lpc-engine` / `lp-view` split (see Q-B) |
| `value_spec.rs` (uses `LpsValue`)      | misplaced            | `lpc-source`      |
| `artifact/`                            | misplaced            | `lpc-source`      |
| `presentation.rs`                      | misplaced            | `lpc-source`      |
| `schema/mod.rs` (Migration trait)      | misplaced            | `lpc-source`      |
| `bus/channel_name.rs`                  | foundation           | `lpc-model`       |
| `tree/tree_path.rs` (TreePath)         | foundation           | `lpc-model`       |
| `tree/child_kind.rs`                   | misplaced            | `lpc-wire`        |
| `tree/entry_state_view.rs`             | misplaced            | `lpc-wire`        |
| `tree/tree_delta.rs`                   | misplaced            | `lpc-wire`        |
| `error.rs` (DomainError)               | foundation           | `lpc-model`       |
| `lp_path.rs`                           | foundation (file paths) | `lpc-model`    |
| `lp_config.rs` (LightplayerConfig)     | foundation? config?  | **see Q-D**       |
| `serial.rs` (baud rate const)          | foundation/utility   | `lpc-model`       |
| `serde_base64.rs`                      | utility              | wherever first consumer is (`lpc-wire`?) |
| `json.rs`                              | utility              | `lpc-wire`        |
| `message.rs`                           | misplaced            | `lpc-wire`        |
| `transport_error.rs`                   | misplaced            | `lpc-wire`        |
| `server/`                              | misplaced            | `lpc-wire`        |
| `project/frame_id.rs`                  | foundation           | `lpc-model`       |
| `project/api.rs` (NodeStatus, ProjectRequest) | misplaced     | `lpc-wire`        |
| `project/config.rs` (ProjectConfig)    | mixed authored+wire  | **see Q-D**       |
| `project/handle.rs` (ProjectHandle)    | runtime?             | **see Q-E**       |
| `state/macros.rs`, `state/test_state.rs` | utility/test       | **see Q-F**       |
| `types.rs` (just module-level docs)    | doc-only             | retire / merge into `lib.rs` doc |

Counts: foundation ~13 files, source ~10 files, wire ~13 files,
plus a handful needing decisions.

## `lp-shader` dependency surface

Files in `lpc-model` that use `lps-shared`:

- `value_spec.rs` — `LpsValue` (artifact-side; portable form)
- `prop/kind.rs` — `LpsType` for `Kind::storage()` projection
- `prop/constraint.rs` — uses `lps-shared` (need to verify)
- `prop/shape.rs` — `LpsType`, `StructMember` for `Slot::storage()`
- `prop/prop_path.rs` — `lps_shared::path::{LpsPathSeg, parse_path}`
- `node/node_prop_spec.rs` — `lps_shared::path` types
- `node/node_props.rs` — `LpsValue` (legacy trait, retires)
- `lib.rs` — re-exports `LpsValue`, `LpsType`, `TextureBuffer`, `TextureStorageFormat`

The path utilities (`lps_shared::path`) are NOT GLSL-coupled — they're
a generic `Field(String) | Index(usize)` parser. The texture and
value/type bits are GLSL-coupled.

## Consumers of `lpc-model`

14 crates depend on `lpc-model` (per `Cargo.toml` grep). After the
split, `lp-view` should depend only on `lpc-model + lpc-wire`
(no `lp-shader`, no `lpc-source`). Other clients (`lpl-model`,
`lpl-runtime`, `lpc-engine`, `lp-server`, etc.) get their dependencies
adjusted to whatever subset they need.

## What's already done that matters

- M4.2 just shipped. New types (`Binding` 3-variant, `NodeConfig`,
  `PropAccess`, `PropNamespace`, `Bus`, `ResolverCache`, etc.) need to
  flow through this split correctly.
- The placeholder doc was [`./plan.md`](./plan.md). It captures the
  framing but doesn't have the file-level audit above.

# Questions

## Triage table — confirmation-style questions

| #   | Question                                                                                  | Suggested answer |
| --- | ----------------------------------------------------------------------------------------- | ---------------- |
| Q1  | Crate names: keep `lpc-model` for shared concepts; add `lpc-source` for authored source files, `lpc-wire` for wire shapes, and `lpc-engine` for engine runtime. | Yes |
| Q2  | Single commit at the end (per `/plan` convention)?                                        | Yes |
| Q3  | The legacy `node/node_props.rs` (`NodeProps` trait that uses `LpsValue`) moves to `lpl-model`, not into one of the new crates — it retires in M5 anyway. | Yes |
| Q4  | The legacy `lpc-model::nodes` module (`NodeSpecifier` alias) is dropped in this milestone — check call sites and update. | Yes |
| Q5  | `prop/prop_path.rs` keeps using `lps_shared::path::LpsPathSeg` (it's a path parser, not GLSL-coupled). Foundation depends on `lps-shared` for that subset only. | Yes |
| Q6  | `WireValue` lives in `lpc-model` (the shared model crate), not in `lpc-source` or `lpc-wire`. | Yes |
| Q7  | `WireValue` is named exactly `WireValue` (not `LpcValue`, `Value`, etc.).                | Yes — emphasizes "this crosses the wire" |
| Q8  | `WireValue::Texture` carries a stable id only in M4.3a; thumbnail/metadata payloads come later as additive variants when the editor needs them. | Yes |
| Q9  | The `LpsValue → WireValue` conversion lives in `lpc-engine` (the boundary owner per the design sketch). | Yes |
| Q10 | The `From<LpsValue> for WireValue` direction is lossy on `Texture2D` (preserves descriptor id, drops storage handle); the inverse is recipe-driven, not direct. | Yes |
| Q11 | `Binding::Literal` switches from `ValueSpec` to `WireValue` as part of this milestone (so `Binding` can move to `lpc-source` without `lpc-source` needing `lp-shader`). | Yes — see Q-C below |
| Q12 | `ValueSpec`'s private `LpsValueWire` mirror retires (replaced by `WireValue`); `ValueSpec::Literal` payload becomes `WireValue`. | Yes |
| Q13 | All design-doc updates needed for type renames (`NodePropRef`→`NodePropSpec`, `LpsValue`→`WireValue` in wire contexts, etc.) happen in the cleanup phase, not interleaved with file moves. | Yes |
| Q14 | Plan version pin in `lib.rs` doc (e.g. workspace version) does **not** change for any moved type. | Yes — semver irrelevance, all `0.x` workspace |

## Discussion-style questions (will surface one at a time)

- **Q-A — `Kind::storage()` and `lp-shader` dependency in foundation.**
  `Kind` is foundation (used by `Binding`, `NodePropSpec` namespace
  checks, etc.) but its `storage() -> LpsType` method projects to a
  shader-runtime type. Three options: (1) keep `Kind` in foundation
  and have foundation depend on `lps-shared` (currently does);
  (2) split `Kind`'s data from `Kind::storage()` — data in foundation,
  storage projection as a free function in `lpc-source`; (3) define
  a `WireType` in foundation that `LpsType` is convertible to, and
  have foundation be `lp-shader`-free. Tradeoffs around dependency
  cleanliness vs. churn.

- **Q-B — Where does `PropAccess` live?**
  The trait is implemented by runtime `*Props` structs (`lpc-engine`
  callers) and consumed by sync (`lpc-wire` callers). It returns
  `LpsValue`-typed payloads today, but should that become `WireValue`
  as part of this milestone? Two questions in one: location and
  payload type.

- **Q-C — `Binding::Literal` payload: `ValueSpec` or `WireValue`?**
  M4.2 just shipped `Binding::Literal(ValueSpec)`. If `Binding` moves
  to `lpc-source` and `lpc-source` should not depend on
  `lp-shader`, then `ValueSpec` (which holds `LpsValue`) is a
  problem. Either `ValueSpec` itself moves to use `WireValue`, or
  `Binding::Literal` switches to `WireValue` directly. (Q11/Q12 lean
  toward "ValueSpec migrates to WireValue payload"; this question
  is whether that's actually right vs. having `Binding::Literal` be
  `WireValue` directly.)

- **Q-D — Where do `LightplayerConfig` and `ProjectConfig` go?**
  These are mixed: authored on disk, read at startup, sometimes
  shipped over the wire (project list). Could go in `lpc-source`
  (authored), `lpc-wire` (wire), or stay in shared model. Lean:
  `lpc-wire` for `ProjectConfig` (since clients receive it),
  `lpc-model` for `LightplayerConfig` (since it's used by the engine
  startup before any wire layer is up).

- **Q-E — `ProjectHandle` location.**
  Looks runtime-flavored (a handle returned by `ProjectRuntime::open`).
  Probably `lpc-wire` if clients use it as an opaque token, or
  `lpc-engine` if it's purely server-side. Need to grep call sites.

- **Q-F — `state/` module.**
  Contains macros and a `test_state` module. Unclear what it's for
  without reading. Probably stays in foundation as utility, or moves
  to a `lpc-wire`/`lpc-model` subset based on actual use.

- **Q-G — Timing: execute now, or plan now and execute after M4.3?**
  The original m4.3a placeholder said "after M4.3 commits." The
  user has started `/plan` for this *now*. Two options: (1) plan now
  + execute after M4.3 (original sequencing — M4.3 inherits the old
  shape and gets cleaned up after); (2) plan now + execute now (M4.3
  inherits clean structure from the start). Tradeoff: option 2 means
  more up-front churn before runtime spine work begins, but a
  cleaner foundation for M4.3. Lean: option 2 — the runtime spine is
  *the* big new consumer, so it should write to the clean
  boundaries from the start.

## Resolved decisions

- Q1-Q14 accepted as suggested on 2026-04-30.
- Q-A accepted: introduce a foundation-side `WireType` / `StorageType`
  so `Kind::storage()` no longer returns `LpsType`; convert to
  `LpsType` only at runtime/compiler boundaries.
- Q-B accepted: split property iteration into `RuntimePropAccess`
  and `WirePropAccess`. `RuntimePropAccess` lives in `lpc-engine`
  and exposes `LpsValueF32`; `WirePropAccess` lives with client view
  code and exposes `WireValue`.
- Q-C accepted: keep `Binding::Literal(ValueSpec)`, but make
  `ValueSpec` use `WireValue` instead of its current private
  `LpsValueWire` mirror. Reuse the existing serde shape and tests
  from `value_spec.rs`; do not create a duplicate parallel value
  representation.
- Crate naming update: replace the previous `lpc-artifact` /
  `lp-artifact` placeholder with `lpc-source`. The user prefers the
  source/src framing because Lightplayer projects are authored source
  code. Use `Src*` type prefixes for persisted authored model types.
- Crate naming update: use `lpc-wire`, not `lp-wire`; this is the
  wire crate of `lp-core`. Use `Wire*` type prefixes there.
- Crate naming update: use `lpc-engine`, not `lpc-engine`; the user
  renamed runtime because engine is clearer for this crate's role.
- Q-D accepted with naming update: authored project/source config
  belongs in `lpc-source`; wire-visible project summaries/views belong
  in `lpc-wire`; `LightplayerConfig` stays in `lpc-model` for now unless
  call-site review proves it is engine-only.
- Q-E accepted: `ProjectHandle` belongs in `lpc-wire` if it appears in
  client requests/responses; otherwise it belongs in `lpc-engine`. Do
  not keep it in `lpc-model` unless call-site review proves it is a
  shared primitive.
- Q-F accepted: move the current `state/` serialization helpers to
  `lpc-wire`. They are used by legacy node state objects such as
  `lp-legacy/lpl-model/src/nodes/output/state.rs`, and their primary
  purpose is partial state serialization for wire updates.
- Q-G accepted: plan and execute M4.3a now, before M4.3 runtime spine
  work continues. Clear crate roles are critical before more runtime
  code grows into the mixed `lpc-model` shape.

# Notes

## Single-concept file preference

The implementation plan should avoid preserving the current
`value_spec.rs` pattern where `ValueSpec`, `ValueSpecWire`,
`LpsValueWire`, `TextureSpec`, TOML parsing helpers, materialization,
serde, and tests are all in one large file. When crates are split,
use granular files/modules with names that line up with the crate and
concept they live in. The user expects to do a cleanup pass after the
boundaries are established, but M4.3a should avoid adding more
multi-concept files.

## Existing `ValueSpec` wire machinery

`lpc-model/src/value_spec.rs` already has most of the desired serde
shape:

- `LpsValueWire` mirrors value variants for serde because `LpsValueF32`
  does not derive serde traits.
- `ValueSpecWire` is an internally-tagged serde form:
  `{ kind = "literal", value = ... }` or `{ kind = "texture", value = ... }`.
- `ValueSpec::Literal` currently stores `LpsValue`; `ValueSpec::Texture`
  stores `TextureSpec`.

M4.3a should reuse that shape by promoting/refactoring the private
`LpsValueWire` concept into the public foundation `WireValue`, not
duplicate it under another representation.

## Naming reconciliation tracking

Across milestones, several aspirational names are being mapped to
existing types or replacements:

| Design name      | Implementation reality                                  |
| ---------------- | ------------------------------------------------------- |
| `NodePath`       | `TreePath` (rename done in M2/M3)                       |
| `NodePropRef`    | `NodePropSpec` (reuse done in M4.2)                     |
| `LpsValue` (wire side) | `ValueSpec` in M4.2; `WireValue` after this milestone |
| `lpc-model` (kitchen sink) | shared model crate after this milestone         |
