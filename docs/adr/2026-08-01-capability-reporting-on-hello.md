# Capability reporting on ServerHello: build facts, hardware facts, and an Unsupported node status

- Status: accepted
- Date: 2026-08-01
- Plan: `2026-08-01-1200-firmware-manifest` (M4)
- Supersedes the deferral recorded in `docs/debt/firmware-capability-reporting.md` (now retired)

## Context

A firmware build links only the node runtimes its Cargo gates enable. A
project referencing a gated-out kind still loads — `ProjectLoader` attaches
`CorePlaceholderNode` — and then does nothing. Nothing on the wire said
why. Worse than silence: the `Output` node is ungated, so every frame the
resolve path warned *"produce: node NodeId(2) does not produce slot
output"* — a plausible **wrong** explanation that reads as an authoring
mistake when the cause is build configuration.

`ServerHello` was designed as exactly this seam
(`docs/adr/2026-07-14-wire-hello-versioning.md`: "fields are added when
they become real"), and M1–M2 of the firmware-manifest roadmap made the
answer derivable: `lpc_model::LpFeature` is the typed registry, and
`lpc_engine::features::supported_features()` derives the engine's enabled
set from the same `cfg!` truth as the gates themselves.

## Decision

### The hello carries build facts and hardware facts, distinctly

`ServerHello` (wire proto **5**) is:

```
ServerHello {
    proto: u32,
    build: BuildFacts { features: Vec<LpFeature>, package, commit, dirty, profile },
    hardware: HardwareFacts { radio: bool, button: bool, board_id: Option<String> },
    device_uid: Option<String>,
}
```

`FwProvenance` is **deleted**, not aliased: its four fields moved onto
`BuildFacts` beside the feature list, because build identity and build
capability are one fact about one binary and splitting them into two hello
fields invited them to drift. The wire-compat posture is version+refuse
(AGENTS.md), so the proto bump is the whole migration; an older device hits
the existing `NeedsFirmwareUpdate` path.

The split that *is* kept is **build vs hardware**. A build's features are
what the image can do; hardware facts are what this unit actually has
wired. They are derived from overlapping inputs today (the service
`Option`s on `LpServer` produce both `svc.*` features and the
`radio`/`button` booleans) but they are not the same question, and a
desktop-class build — GPU present or not at runtime, same binary — makes
the distinction load-bearing. `board_id` is present and always `None`:
nothing on the device writes a board identity yet, and the field exists so
that becomes a population change rather than a wire change when
provisioning writes `/hardware.json` (board-selection roadmap M5).

### Features are reported, not node kinds

`build.features` is a `Vec<LpFeature>`, not a list of supported
`NodeKind`s. A client derives supported kinds through
`LpFeature::for_node_kind`: `None` means the kind is never gated
(`Project`, `Output`) and is therefore always available; `Some(f)` means
available iff `features` contains `f`. One vocabulary describes node
runtimes, services, graphics backends and shader math, and it is the same
vocabulary the embedded manifest core uses — hello is a *projection* of
build truth, never a second statement of it.

### Capabilities are computed in the constructor, never injected

`LpServer::new_with_hardware_services` computes `build.features` and
`hardware` from `lpc_engine::features::supported_features()`, the injected
service `Option`s, and `LpGraphics::backend_name()`. The embedder supplies
only what the server cannot know, through `set_hello_identity(HelloIdentity)`
— package, commit, dirty, profile, uid, proto — and **that call cannot
reach the capability half**. This is the point of the reshape: `fw-emu` and
`lp-cli` never state an identity at all, and under the old
"embedder-injects-the-whole-hello" shape they would have reported an empty
or wrong capability list. Deriving capabilities where the inputs live makes
forgetting impossible.

The one exception is declared, not guessed: `shader.f32` is a property of
the embedder's Cargo graph (`float-f32`), invisible from anything
`LpServer` holds, so an embedder that has it calls
`declare_embedder_features(&[LpFeature::ShaderF32])` at construction — the
same shape as the manifest macro, where the embedder likewise names only
its own facts. `fw-esp32s3` is the only caller today.

### A gated-out node says so: `NodeRuntimeStatus::Unsupported`

`CorePlaceholderNode` reports `Unsupported("node kind X is not included in
this firmware build")` — but only for kinds that *can* be gated
(`LpFeature::for_node_kind(kind).is_some()`). The project ROOT is also a
placeholder, spelled `new_leaf(NodeKind::Project)`, and `Project` is
always-on: the registry draws the line, so the root stays quiet.

`Engine::attach_runtime_node` now adopts a runtime's self-reported status
at attach time, so the status rides the FIRST `WireTreeDelta::Created` a
client sees rather than appearing only after some later change. Runtimes
that report nothing (the trait default) leave the entry untouched.

The misleading resolve warning is reframed at its source: when the
producing node's status is `Unsupported`, the `ProduceResult::Unsupported`
arm reports the build gap by name. Genuine wrong-slot diagnostics on real
nodes are unchanged, word for word.

### Studio: "Not on this device", dimmed — never red, never violet

`Unsupported` maps to a new `ProjectNodeStatusTone::Disabled` carrying the
label **"Not on this device"**. It projects onto `UiStatusKind::Neutral`,
so the affordance merge keeps it at `Info` — silent chrome, no attention
glyph — and the meaning is carried by the status WORDS plus a dimmed
treatment on the tree row and the node pane. It is deliberately not the
error family (nothing is broken) and deliberately not violet (reserved for
bound). A new `UiStatusKind` variant was considered and rejected: it would
have dragged `PaneTone`, `DetailSectionTint`, six exhaustive matches and a
new CSS token family along for one status that renders in one place.

The device card's Technical section reports capabilities **gaps-only** — a
fully-capable device adds no lines at all, because listing what every board
has would bury the one line that matters on the board that lacks something.
The add-node picker **disables, never hides**, kinds the connected device
lacks; a picker that drops entries teaches a false catalog. Gating narrows
only when a device affirmatively reports: a sim/host lens, or a link that
is not `Ready`, leaves everything enabled.

### This is orthogonal to protocol versioning

`docs/adr/2026-07-14-wire-hello-versioning.md` rejects "no minor/patch
structure and no capability list" **for versioning**, and that line stands
unchanged. The feature list describes runtime abilities; it is never
consulted to decide whether two peers can talk. There is still exactly one
integer, compared for equality, and still no negotiation, no graceful
degradation, no per-message compatibility matrix. If a capability check
ever gates a *message*, this ADR has been violated.

## The schema-entanglement claim was stale

The retired debt entry asserted that any signal routed through
`NodeRuntimeStatus` was "simultaneously a wire change and a generated-schema
change", gated by the `format:1` conformance check — and that assertion is
why the honest fix was deferred rather than shipped in pieces. It is wrong.

Evidence: adding the `Unsupported` variant left `schemas/` **byte-identical**
(`just schema-check` green, no regeneration). Schema generation is
slot-shape-registry driven and invokes `schemars::schema_for!` for exactly
two types (`HardwareManifestFile`, `BoardDisplayFile`); `NodeRuntimeStatus`
appears in no file under `schemas/`, so its `schema-gen`-gated `JsonSchema`
derive is orphaned. No `format:N` bump was needed or taken. What *would*
move the schemas is a new `NodeKind` or `NodeDef` variant — not a status
variant.

The derive stays (behind `schema-gen`, like its siblings) so `schemars`
never enters an RV32/Xtensa graph.

## Consequences

- Every flashed device reports `NeedsFirmwareUpdate` until reflashed at
  proto 5 — correct under the lockstep policy, and the existing UX handles
  it.
- `fw-emu` and `lp-cli` report correct capabilities for the first time,
  without either of them learning about hello.
- The three `manifest-core.expected.json` fixtures move 4 → 5; CI's
  extract-and-diff job re-proves them against real builds.
- A new `LpFeature` variant remains a compile error at every classifying
  site, now including the graphics-backend mapping in `lpa-server`.
- Gated-out kinds are covered by `disabled_node_kind_still_loads_project`,
  which runs under **no CI job** (nothing tests `lpc-engine` with a
  non-default feature set). A no-default-features lane stays future work.

## Rejected

- **Keeping `fw` beside a new capabilities field** — two build-identity
  fields on one frame, free to disagree.
- **`node_kinds: Vec<NodeKind>` on the hello** — a second vocabulary for
  the same truth, and no place for services, graphics or shader math.
- **Injecting capabilities via `set_hello`** — the shape that would have
  shipped wrong lists from the two embedders that never call it.
- **A new `UiStatusKind::Disabled` tone family** — honest, but a
  design-system-wide cascade for one status; copy plus dimming carries it.
- **Hiding unsupported kinds in the picker** — silent narrowing with no
  place to explain itself.
- **Capability-based protocol negotiation** — permanently rejected while
  the alpha wire posture holds; see above.
