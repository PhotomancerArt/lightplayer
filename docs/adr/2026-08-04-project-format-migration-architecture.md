# ADR: Project-format migration architecture

- **Status:** Accepted
- **Date:** 2026-08-04
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates to:** `2026-07-05-artifact-format-version-and-schema-snapshots.md`
  (amended by this ADR — decisions 5–6 are hereby implemented),
  `2026-07-28-share-envelopes.md` (annotated by this ADR — packages now
  migrate on import, nodes are stamped), `../debt/library-format-migration-gap.md`
  (this plan's acceptance criteria were its exit criteria), plan
  `planning/2026-08-04-1800-project-format-upgrades` (PR #344)

## Context

The project format bumped four times in a month (1→2 `float_mode`, 2→3
project/module mitosis, 3→4 endpoint→channels, 4→5 TimeProduct), and every
bump so far meant hand-rewriting the repo-local authored corpus. That did
nothing for a user's OPFS library on lightplayer.app or a project already on
a fielded board: `ProjectRegistry::check_container_manifest` (`lp-core/lpc-
registry/src/registry/project_registry.rs`) refuses a format mismatch, and
three swallow points upstream of it — `LibraryStore::list()`'s log-and-skip,
`hydrate_home_inputs`'s filter_map drop, and `slug_for_uid`'s "not found" —
meant an old-format package simply vanished from the gallery with only a
`log::warn!` as its trace. On a board, an old-format project boot-failed
with one warn line and the roster's only affordance was Wipe.

Two things made this the moment to fix it rather than defer it further:
Yona hitting the silent-failure pain directly on lightplayer.app, and the
first real external users (Zook, Spencer — a 16', 1500-LED dome) arriving
the following week, with the dome's own parametric-mapping work likely to
force another format bump before their build finishes. `docs/adr/2026-07-28-
share-envelopes.md`'s "version and refuse, never migrate" ruling was
explicit that the posture was temporal ("too heavy devving *right now*"),
and its own debt doc named the trigger that fires this work: fielded
devices holding old-format projects that must survive a breaking bump.

The reframe that grounds every decision below: **migration tooling is not a
step toward freezing the format — it is what permits continued breaking
changes.** A studio-side upgrader means the format keeps moving at
development speed without leaving real users' projects behind.

## Decision

### 1. `lp-app/lpa-upgrade`, host + wasm, never firmware — enforced by a lint

The crate lives at `lp-app/lpa-upgrade`, not `lp-core` (the vision's first
guess, `lpc-upgrade`, was wrong): `lp-core` is in the firmware dependency
graph (`lpc-model`/`lpc-registry` reach `fw-core` via `lpa-server`), so
anything living there is reachable from firmware regardless of feature
gating discipline. `lp-app/lpa-upgrade` sits beside the other host-only
`lpa-*` crates. It builds for `wasm32-unknown-unknown` and host, and is
sans-IO (AGENTS.md): no filesystem, no clock, no randomness — callers hand
it a `path → bytes` map and get one back. Firmware exclusion is enforced,
not asserted: `scripts/check-upgrade-fw.sh` (the `check-schemars-fw.sh`
pattern — an inverted `cargo tree -i lpa-upgrade` check against the RV32
firmware graphs) is wired into `just check-lint`.

The device itself never migrates a project — that was already decided
(`2026-07-05-artifact-format-version-and-schema-snapshots.md`, decision 5):
an upgrader on the ESP32 would cost flash, RAM, and complexity for a job
that never needs to run there. This ADR is where that decision actually
gets built out.

### 2. Behavior preservation, not improvement — phasor-ization stays authoring

A migrated project does exactly what it did before; it does not do it
*better*. The concrete case: the v4→v5 hand migration (done by a human, in
git history) converted several `bus:time` uniforms into phasors — new slot
names, periods mined out of the GLSL, labels — information that simply is
not present in the v4 bytes. An automated step that invented that would be
authoring passed off as migration, wrong in ways nobody would notice for
months. So `lpa-upgrade`'s v4→v5 step does the behavior-preserving half
only: an f32 `bus:time` uniform becomes a `seconds` slot (same number, same
GLSL, same rendered output); the clock's own `bus:time` binding is
retargeted `seconds` → `product`. Turning a `seconds` slot into a `phasor`
with mined periods stays a human or agent authoring task, run separately,
any time after the migration.

### 3. Chained vN→vN+1 plain-code steps over an order/text-preserving JSON tree

Migrations are Blender's `do_versions`, not Minecraft's DataFixerUpper: one
plain Rust function per version bump (`src/steps/v4_to_v5.rs`, registered in
`src/steps/mod.rs::STEPS`), run in a dense chain — `upgrade_to_current`
walks every step from the project's found format through to
`PROJECT_FORMAT_VERSION`, and refuses if the chain has a gap. No generic
migration framework, no optics/lens abstraction: the architecture is worth
stealing, the abstraction is not (Minecraft's DataFixerUpper was raised
explicitly as the cautionary tale).

Steps operate on `JsonNode`, a small (~150-line) hand-rolled order- and
text-preserving JSON document tree (`src/json/json_node.rs`) — **not**
`serde_json::Value`. Two properties `Value` cannot give together forced
this: `serde_json::Map` is a `BTreeMap` unless the crate-wide
`preserve_order` feature is enabled, and Cargo feature unification is
per-build, not per-crate — turning it on for this crate would turn it on
for every host crate in the same build, including `schemars`, whose
generated `schemas/` output is committed in sorted order and gated by `just
schema-check`. A migrator is not allowed to churn the schema corpus as a
side effect of its own dependency graph, so order lives in `JsonNode`
instead. Second, `Value` stores numbers as `f64` and re-emits them through
ryū, so `0.00003` in a real fixture (`fluid.json`) would silently become
`3e-5` on any file the migrator touched — `JsonNode` keeps every scalar as
its original source text. `json_file_edit::edit_json_files` applies an edit
to every `*.json` file and rewrites **only the ones that actually changed**
— the authored corpus is not canonically formatted (indentation, inline vs.
expanded records vary file to file), so a step that re-serialized untouched
files would produce a diff no human could review.

Steps key off *meaning* — a binding's source being `bus:time`, a shape's
structure — never off a field's *name*. The v4→v5 step's own doc calls this
out as "rule R10, in the negative": `fyeah-sign/blast.json` has a slot named
`time` bound to `node:..#entry_time` that must pass through byte-identical,
and a name-keyed rule would have mangled it. Anything a step does not
recognize is `UpgradeError::Refused`, never guessed at; a run is
all-or-nothing (a refusal on the last file leaves the first untouched), so a
caller never has to reason about a half-migrated package.

### 4. Support floor v4, honest refusal below it

`UPGRADE_FLOOR = 4` — this format and one prior (pre-TimeProduct). Formats
1–3 predate project/module mitosis; their types are long deleted and their
only remaining trace is `schemas/history/`'s frozen snapshots. Below the
floor, above the current version (a project written by a newer
LightPlayer), or a shape a step does not recognize: every one of those is a
distinct, describable `FormatClass` (`src/format_class.rs`) whose `describe()`
names what was found, what this build expects, and the remedy — sniffed
*before* any parse that could fail for an unrelated reason (the same
`peek_header_lenient`-style classify-first-parse-second shape share
envelopes already use), because the strict `ProjectManifest::read_json`
hard-errors on a pre-mitosis manifest's unknown top-level keys before
anything gets to look at `format`. Raising the floor later is a deliberate
act — delete the steps and corpus below it, move the constant — not an
automatic consequence of anything in this design.

### 5. Goldens are the migrator's own checked-in expected output, byte-exact; changed-files-only rewrites

`migrate(v4 fixture)` does not equal the current hand-polished corpus —
the hand migration went past behavior preservation into phasor authoring
(decision 2). So the goldens under `tests/corpus/v4/_expected/` are **this
crate's own** output, human-reviewed once via `LPA_UPGRADE_BLESS=1 cargo
test -p lpa-upgrade --test corpus_goldens` and then frozen as a byte-exact
regression test — not a comparison against `examples/`/`projects/test/`.
Two more tests keep a golden from being a rubber stamp: every migrated
project must load through the real `ProjectRegistry` (the "a writer whose
output no reader consumes in tests is an unverified contract" lesson from
`docs/defects/2026-07-27-created-package-unloadable.md`), and every
*unmigrated* fixture must fail to load (a golden that was never broken
proves nothing). The corpus itself is real format-4 projects (the two
frozen `schemas/history/v4/fixtures/` snapshots plus four gallery examples,
GLSL and SVG recovered from `f9d6981dc^` because the pre-P6 `format-bump`
recipe only snapshotted `*.json` — fixed under Consequences below, so the
*next* bump's fixtures come from the snapshot directly).

### 6. Auto-migrate on library open, non-blocking notice, write-back before hash

Opening a package that classifies as `PackageHealth::UpgradesOnOpen`
migrates it in place before the open proceeds
(`project_controller.rs::migrate_package_on_open`) — no confirmation
prompt, an informational `UiNotice` only ("Upgraded \"…\" from format N to
M"). The order is forced by `open_library_project`'s hash check
(`studio_server_client.rs`): the migrated bytes must be written and
`record_save`d *before* anything reads the package for the hash compare, or
the runtime would push bytes the library handle does not yet have on disk.
`record_save` is the ordinary save/history path, not a bespoke migration
event — which is deliberate: it is also, for free, the undo path. The
pre-migration bytes are the previous history version, exactly as any other
edit would bank them; a migration is a normal save whose diff happens to be
machine-written. A package this build cannot migrate is refused with a
classified `UiIssue` (headline + remedy) carried on `ProjectState::Failed`,
replacing the raw parser string that used to be the only thing that landed
there — the whole reason `RegistryError::FormatVersion` had to stop being
stringified before it left the registry layer. A package the library cannot even classify no longer vanishes: it
gets a slug-derived stand-in identity (`derived_uid`) so it can still show
a card and support delete, even with an unreadable or absent `uid`.

### 7. Device flow: pull → migrate-in-the-library → push, never in place

The roster's Upgrade verb never migrates the board. It pulls (which already
banks the board's bytes into the library at connect, D8 — no read is ever
lost), migrates the **library package**, and pushes the migrated bytes back
over the ordinary hash-checked push path. The landing decision on *which*
bytes are the migration subject: when the board's project already resolves
to a library package (the common case — connect-is-a-pull already adopted
or matched it), that library package migrates, not the pulled copy. This
means the verb can never let an older board copy clobber a newer local
head — that is `Use board copy`'s job, a decision a human makes, not one an
upgrade silently makes for them. Only when the library holds no copy at all
does the pulled copy itself get adopted and migrated, mirroring the shape
connect already uses. `library/package_upgrade.rs::migrate_handle_to_current`
is the one migration-and-save function both the library-open path (decision
6) and the device-upgrade path share — write every changed file back through
`apply_update`, then `record_save` — so there is exactly one place, not two,
that has to get the "migrate before hash" ordering right.

An associated board legitimately reads Diverged after a library-side
migration, until the next push resolves it — accepted as-is (G1 ruling),
not treated as a defect requiring new copy for the card.

### 8. `lp.node` share envelopes carry an `artifact_format` stamp; unstamped/mismatched nodes refuse, they do not migrate

`NodeEnvelope` gains `artifact_format: Option<u32>`, stamped with
`PROJECT_FORMAT_VERSION` on every new export. `Option`, not a defaulted
`u32`: a pre-stamp envelope (`None`) and a stamped-but-mismatched one need
distinguishable messages, and a defaulted value would have collapsed that
distinction. Neither case is migrated this round — bare-node migration
needs the stamp to exist everywhere first, which this round cannot
guarantee, so both refuse loudly with the classifier's own sentence. This
also resolves two standing debt items in the same stroke: zip import used
to install with no format check at all, and `lp.node` envelopes carried no
engine-format version whatsoever. Zip import and envelope-paste import both
now `gate_and_migrate` — classify, migrate if this build can, refuse
otherwise — **before** `install_package`, which stays byte-faithful by
design (`library_store.rs`); migration always happens *before* install,
never inside it.

## Alternatives Considered

- **Declarative shape-dump-driven rules.** The shape registry
  (`schemas/shapes/`) already exists and is tempting to drive migrations
  from directly. Rejected: real bumps are semantic (TimeProduct retyped
  `bus:time` consumers based on which *channel* a binding named, not any
  shape difference schemas can express), and a declarative diff would not
  have expressed three of the four historical breaks. Shape dumps and
  fixtures stay test oracles — what a step's output must load against —
  not the migration language itself.
- **A DataFixerUpper-style optics/lens framework.** Minecraft's is the
  canonical example: architecturally sound, but the abstraction (profunctor
  optics over a generic value tree) is heavy machinery for a chain that, at
  launch, has exactly one real step. Blender's `do_versions` — plain
  imperative functions, one per bump — is the model actually adopted.
- **A tolerant reader** (accept and best-effort-interpret whatever shape
  shows up). Already rejected once, for the artifact codec itself
  (`ProjectManifest::read_json`'s unknown-field strictness is what makes
  read→modify→write lossless) and not reopened here: a tolerant migrator
  would silently misinterpret exactly the projects most likely to need
  careful handling.
- **Typed version chains** (serde-version/obake-style: a Rust type per
  historical version, converted through `From` impls). Rejected because old
  model types are genuinely deleted at each bump under this repo's
  heavy-dev churn policy — keeping N-deep typed history alive across every
  future model refactor is a maintenance tax nobody signed up for. Untyped
  JSON trees don't care that the old types no longer compile.
- **Kubernetes-style hub-and-spoke** (every version converts to/from one
  central "storage" version). That architecture earns its complexity when N
  versions are simultaneously *live* (multiple API clients, one server).
  This repo has exactly one live version and a dead chain behind it —
  chained vN→vN+1 is the whole problem, correctly sized.
- **Migrating on the device.** Already excluded by
  `2026-07-05-artifact-format-version-and-schema-snapshots.md` decision 5;
  restated here because this ADR is where it is actually enforced by a
  lint rather than left as a stated intent. An on-device upgrader would
  cost flash and RAM for a job Studio can always do first.

## Consequences

- The next format bump cannot land without its migration step: `lpa-upgrade`'s
  `the_chain_ends_at_the_current_format` test asserts the step chain's tip
  equals `PROJECT_FORMAT_VERSION`, so a bump without a step fails `cargo
  test -p lpa-upgrade` — and CI — the moment the constant moves. A companion
  test (`the_current_format_has_a_history_snapshot`) fails the same way if
  `schemas/history/v<N-1>/` is missing, closing the other half of the
  bump-implies-snapshot follow-up below.
- `just format-bump` now snapshots fixture project directories verbatim
  (every file, not `*.json` only — the gap that forced this plan's P1 to
  recover GLSL/SVG from `f9d6981dc^` by hand) and scaffolds the next step
  file, so the ritual and the crate's own enforcement point at each other.
- A format bump remains a real engineering event — writing the step,
  growing the fixture corpus, blessing and reading the goldens — not free.
  What changed is that skipping it is now a build failure, not a silent gap
  discovered by a user.
- Bare-node migration and v1–v3 support remain explicitly out of scope
  (decisions 4 and 8) and are debt with named triggers, not forgotten work
  — see `../debt/library-format-migration-gap.md`.
- The safe-mode board rescue hole — upload cannot reach a safe-mode board,
  so a board wedged in safe mode holding an old-format project cannot
  complete pull→migrate→push — is unresolved by this plan and is registered
  as its own debt entry with a field-occurrence trigger
  (`../debt/safe-mode-board-rescue-hole.md`).

## Follow-ups

- Bare-node migration (needs the `artifact_format` stamp to be universal
  first — decision 8). Trigger: a real user is stopped by a stale node
  paste.
- v1–v3 migration, below the current floor (decision 4). Trigger: a real
  holder of pre-v4 project data appears.
- Safe-mode board rescue hole — upload cannot reach a safe-mode board, so
  decision 7's flow has no push leg there (see Consequences). Trigger:
  first field occurrence of a safe-mode board holding an old-format
  project. `docs/debt/safe-mode-board-rescue-hole.md`.
- Automated additive-vs-breaking format classification remains deferred
  (unchanged from `2026-07-05-artifact-format-version-and-schema-snapshots.md`
  decision 6) — this plan gave the format a migration path, not a
  compatibility classifier.
