# ADR: `target` is advisory board metadata, not a mechanism assertion

- **Status:** Accepted
- **Date:** 2026-08-05
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-08-03-multi-endpoint-output-node.md` (refines, does not
  reverse — see below), `2026-08-01-project-module-mitosis-container-format-gate.md`
  (the container manifest this field joins), gallery-rework
  `vision.md` D3

## Context

A project's `output` nodes already name real device endpoints
(`ws281x:local:IO18`), so a project is board-shaped in every way that
matters *except* that nothing records which board that shape targets. The
gallery-rework plan (P02) needs that fact in three places: the board-aware
project generator (P03) has to pick sensible defaults for *some* board when
it writes a new project; the sim needs to inherit a board identity from the
project it loads (P04); and a project card can tell the user which board a
project was built for before they load it onto a different one (P06's
mismatch warning, deferred but the badge lands now).

None of that requires the engine to know anything. The multi-endpoint ADR
already drew this exact line for the *output* node: an endpoint spec names
a device and a wire, never a driver mechanism, because firmware picks the
mechanism from the board manifest. `target` draws the same line one level
up, for the whole project: it names *which* board, never *how* to drive it.
Recording the implicit (a project's endpoints already presuppose some
board) as an explicit, advisory field is what makes the mismatch warning
possible without asking the engine to interpret it.

## Decision

**`ProjectManifest` gains `target: Option<String>`** (`lp-core/lpc-model/src/project/manifest.rs`),
a board catalog id in the registry's existing `vendor/product` vocabulary —
the same strings `RegisteredDevice.board_id` already carries (e.g.
`espressif/esp32-c6-devkitc-1`). It sits beside `author`/`license`/`created`
as provenance-tier metadata, not beside `uid`/`name` as identity: absent by
default, never required, never validated against the board catalog (`lpc-model`
carries no catalog dependency and never will for this field).

**The engine never reads it.** No engine or runtime code path touches
`target` — not generation logic (P03 writes it, but writing is a library/
generator concern, not an engine one), not load, not render. It exists
entirely for Studio-side advisory purposes: generation defaults, sim board
inheritance, and the load-time mismatch warning (P06, out of this phase's
scope).

**Surfaced read-only, quietly.** A project/package card with `target` set
shows a neutral "for \<board\>" badge (`lp-app/lpa-studio-web/src/app/home/package_card.rs`),
resolving the raw id to the catalog's `display_name` when the board is
known and falling back to the raw id otherwise (advisory data may name a
board a given build's catalog doesn't carry — the badge should still say
something rather than disappear). The badge is deliberately the same
neutral tone as the card's other facts; a warn tint is P06's job, in
mismatch context only, and no such context exists yet.

### Not a format bump

`PROJECT_FORMAT_VERSION` stays **4**. The container's streaming reader
(`ProjectManifest::read_json`) is hand-rolled specifically to be strict —
unknown top-level keys are a hard, loud parse error, not a silent drop —
which is what makes the container's read→modify→write patching lossless.
That strictness, not the format number, is what already protects against
the failure mode a version bump exists to prevent: a build that predates
`target` support refuses a project.json carrying it outright, with a clear
diagnostic, rather than silently discarding the field or misinterpreting
it. This is exactly the precedent set when `author`/`version`/`license`/
`created` joined the container earlier in format 4 (project/module mitosis
P3): a purely additive, optional container field, never touching the
*meaning* of any field that already exists, added without a bump.

Contrast the actual format-4 bump (3 → 4, multi-endpoint output nodes):
that change repurposed an *existing* field's semantics (`OutputDef.endpoint`,
one string, into `channels`, a map) inside node artifacts, whose codec is
lenient by design (missing fields default rather than error). A stale
reader there would have silently produced a dark, misconfigured node —
the single worst failure mode the version-and-refuse posture exists to
rule out. `target` changes no existing meaning and lives in the one
artifact whose parser already refuses unknown vocabulary outright, so the
version gate has nothing further to add here. (The mechanical cost also
argues the same way: an actual bump requires updating every checked-in
`project.json` fixture across `lp-core`, `lp-app`, and `lp-fw` — dozens of
files with no relationship to targeting — which is proportionate to a
real breaking change, not to one additive, engine-invisible field.)

Reserve the bump for the next change that alters what an *already-valid*
field means, not for the next additive one.

## Consequences

- `target` round-trips through the manifest's canonical writer like every
  other optional field (present → written; absent → omitted entirely),
  keeping unrelated projects' `project.json` byte-identical.
- `schemas/project.schema.json` (generated, `additionalProperties: false`)
  declares `target` as an optional string. The generator's `author`/
  `version`/`license`/`created` gap (those `ProjectManifest` fields are not
  yet declared in the schema) is pre-existing and untouched by this
  change — flagged, not fixed, here.
- `UiPackageCard.target` and `ManifestFields.target`
  (`lp-app/lpa-studio-core/src/app/library/package_manifest.rs`) pass the
  raw board id straight through; the friendly-name lookup lives only in
  the web renderer, so the model and core view layers stay catalog-free.
- The generator (P03) and the mismatch warning (P06) are the field's real
  consumers; this phase only lands the metadata and its quiet badge.

## Alternatives Considered

**Bump `PROJECT_FORMAT_VERSION` to 5 anyway, "to be safe."** Rejected: the
container's closed-vocabulary parser already turns an unaware reader's
encounter with `target` into a loud refusal, which is the *exact* property
a bump exists to buy. Bumping would have bought nothing beyond what the
parser already guarantees, at the cost of updating every fixture project
in the repo (registry, engine, server, firmware, and CLI test suites) for
a field none of them touch.

**Validate `target` against the board catalog at the model layer.**
Rejected: `lpc-model` is the no-catalog core the whole `lpc-hardware`/
`lpa-boards` split exists to keep that way (device firmware links
`lpc-model`, never the catalog). Advisory data does not need — and must
not require — a catalog dependency to parse.

**Put the friendly board name on the manifest instead of the raw id.**
Rejected: the raw `vendor/product` id is what the registry already uses
for `RegisteredDevice.board_id` and what the mismatch warning (P06) will
compare against; a display string is a presentation concern that changes
with catalog updates and belongs at render time, not baked into authored
data.

## Follow-ups

- P03 (board-aware generator) writes `target` when it generates a project
  for a specific board.
- P04 (sim board identity) reads a project's `target` to show "as \<board\>"
  on the sim card.
- P06 (setup wizard) adds the load-time mismatch warning when a project's
  `target` disagrees with the board it is being loaded onto — the first
  place `target` gets a non-neutral (warn) treatment.
- The schema generator's `author`/`version`/`license`/`created` gap noted
  above is out of scope here; flagged for separate cleanup.
