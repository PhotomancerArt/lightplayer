# lpa-upgrade

Offline format upgrades for authored LightPlayer projects.

`schemas/history/` freezes the outgoing format at every bump. This crate is
the thing that finally *reads* those snapshots: it classifies what format a
project on disk is at, and migrates it forward to `PROJECT_FORMAT_VERSION`
through a chain of per-version steps.

Host and `wasm32` only. Sans-IO: no filesystem, no clock, no randomness — the
caller hands it a `path → bytes` map and takes one back.

## The contract

**Behavior preservation.** A migrated project does exactly what it did
before. It does not do it *better*. The v4→v5 hand migration of the gallery
converted several `time` uniforms into phasors, with periods mined out of the
GLSL and new slot names and labels — information that is simply not present
in the v4 bytes. An upgrader that invented those would be authoring, and it
would be wrong in ways nobody would notice for months. So a `bus:time` f32
uniform becomes a `seconds` slot: same number, same GLSL, same output.
Phasor-ization is polish, and it stays a human's job.

**Minimum churn.** Only files a step actually changed are rewritten (2-space
pretty, trailing newline, authored key order and numeric spelling preserved).
Everything else — GLSL, SVG, mappings, artifacts with nothing to migrate —
comes back byte-for-byte identical. The authored corpus is not canonically
formatted (`projects/test/zook-dome-1500/project.json` is 1-space indented,
`0.00003` must not become `3e-5`), and a migration diff a human cannot read
is a migration a human cannot review.

**Loud refusal.** Below the floor, above the current version, or a shape a
step does not recognize: every one of those fails with a message naming what
was found, what was expected, and a remedy. Silent failure is the problem
this crate exists to end.

## The floor

`UPGRADE_FLOOR = 4` — this format and one prior. Formats 1–3 predate
project/module mitosis; their types are deleted and their corpus is three
snapshots in `schemas/history/`. They are refused with an explanation, never
guessed at.

Raising the floor is a deliberate act: delete the steps below it, delete
their corpus directories, and move the constant. The refusal message is the
user-visible contract, so change it in the same breath.

## Adding a step at the next bump

A format bump without a step is a bump that breaks every project anybody
already authored. `upgrade::tests::the_chain_ends_at_the_current_format`
fails CI the moment `PROJECT_FORMAT_VERSION` moves past the chain tip, which
is the reminder mechanism. The ritual, alongside `just format-bump`:

1. `just format-bump` snapshots the outgoing format into
   `schemas/history/v<N>/` **before** the constant moves.
2. Copy the fresh fixtures into `tests/corpus/v<N>/<project>/` — the whole
   project, assets included. Add any authored project that exercises a shape
   the fixtures miss.
3. Write `src/steps/v<N>_to_v<N+1>.rs` with an `apply` function, and register
   it in `src/steps/mod.rs::STEPS`. Plain functions over `JsonNode`; no
   framework. (Blender's `do_versions`, not Minecraft's DataFixerUpper — the
   architecture is worth stealing, the abstraction is not.)
4. Key off *meaning*, never off a name. The v4→v5 step keys off `bus:time`
   references, because `fyeah-sign/blast.json` has a `time` uniform bound to
   a playlist entry's elapsed time that must pass through untouched.
5. Refuse rather than guess. Every shape the step does not recognize should
   reach `UpgradeError::Refused` with a reason a human can act on.
6. Bless and **read** the goldens (below).
7. Run the crossing walk: the next firmware update upgrades every board's
   stored projects with this same step (below, "Who calls it").

A step that changes what a project *does* only through arithmetic the JSON
does not show (where a lamp lands, how many cells a shape holds) needs a
proof that does not share that arithmetic: record the live engine's answer
before the bump, as v11→v12's landing goldens did (below).

## Corpus and goldens

- `tests/corpus/v4/<project>/` — real format-4 projects. The two frozen
  `schemas/history/v4/fixtures/` snapshots, with the GLSL and SVG that
  snapshot dropped recovered from `f9d6981dc^`, plus four gallery examples
  recovered whole from the same commit.
- `tests/corpus/v4/_expected/<project>/` — what this crate produces. These
  are **our** contract, human-reviewed once and frozen thereafter. They
  deliberately do not match today's `catalog/` and `projects/test/`, which
  were hand-polished past behavior preservation.
- `tests/corpus/v5/<project>/` — real format-5 projects: the two frozen
  `schemas/history/v5/fixtures/` snapshots (`quad-strips-v3`, a bare
  single-shader project; `fyeah-sign`, multi-shader), plus `basic` (pulled
  from `projects/test/basic/`), chosen because its `shader.glsl` has two comments
  that literally contain the word `render` next to the entry — proof the
  signature-anchored rename does not touch comment text.
- `tests/corpus/v5/_expected/<project>/` — same contract as the v4 goldens.
- `tests/corpus/v6`, `v9`, `v10` — the bump's two fixture snapshots (plus
  `button-sign` at 10), each carried by the whole remaining chain.
- `tests/corpus/v11/<project>/` — the format-11 projects the v11→v12 step
  was built against: the two fixture snapshots, the projects whose mapping
  says something the step must preserve (peach-1d's lamp-range rows,
  small-dome's two named outputs and rotations, eaves-wired's manual flow
  and reversed row, playful-choker's id-less objects, logo-sign's colliding
  name slugs, fiber-headband's lamps listed in its def, zook-dome, a
  format-6 map2d that must pass through: shader-oracle-6), and two
  synthetic ones (`nested-repeat`, `two-outputs-one-channel-auto`).

Every corpus version is checked by **one** test file, `corpus_goldens.rs`:
each project is carried through the whole remaining chain and must equal its
`_expected` tree byte for byte, with the report's file lists exact. Checks
specific to one step sit beside it as named tests in the same file (nothing
still references `bus:time`; nothing still pins `fixed`) or in that step's
own test file (`step_v11_to_v12.rs`). A format bump touches every existing
corpus version's goldens (re-bless and read the diff), not just the new one.

Regenerate after an intentional change:

```bash
LPA_UPGRADE_BLESS=1 cargo test -p lpa-upgrade --test corpus_goldens
```

then read every line of `git diff` before committing. A blessed golden nobody
read is a regression test for the bug.

Two tests keep the goldens honest: every migrated project must load through
the real `ProjectRegistry` (its root module and every node `Loaded`, not just
`project.json`), and every *unmigrated* one must fail to (a golden that was
never broken proves nothing).

**The file set.** A step may add and remove files (v11→v12 deletes patch
documents and adds a map2d per hand-placed fixture). `UpgradeReport` names
them in `added_files` and `removed_files` beside `changed_files`, and
`untouched_files_come_back_byte_identical` asserts the set changes exactly
as reported and every other file comes back byte for byte. A caller applies
all three lists.

## v11 → v12: the mapping rebuild

Project format 12 moves every fixture's mapping and patching into one map2d
format-6 file (`lpc-mapping`'s parts, segments and patcher; ADR
`docs/adr/2026-10-27-mapping-model-parts-segments-patcher.md`, amendment
"the upgrade"). `src/steps/v11_to_v12.rs` freezes where every lamp lands
today into a fixture start and overrides, mints ids, names the outputs a
start names, writes the map2d at format 6 and removes the patch document;
its module doc is the full list, `tests/refusals.rs` the projects it
refuses by name.

`src/v11/` is a **frozen port** of the format-11 arithmetic the step needs
(lamp counts, patch rows, auto-flow, the default output, port slicing) onto
`JsonNode`, the filled polygon's cell rule copied verbatim with its source
commit named, `libm` for the same trig. It is never edited to match a later
engine: it is what format 11 did. The crate still depends on no current
model; `lpc-mapping` is a dev-dependency, used by the tests to read what the
step wrote.

**Landing goldens** (`tests/landing/<project path>.landing.json`): before
the bump, the live format-11 engine recorded where each lamp of every repo
and corpus project landed (output node, port key, channel). They are
**never re-blessed**: the recorder was deleted with the bump, and they stay
as the record of format 11. Two tests read them, and share no code with
each other: `lpc-engine`'s `upgrade_landing` (every migrated project, loaded
at format 12, lands every lamp there: 117 projects, 66,654 lamps, 0
differences) and this crate's `frozen_landing` (the frozen arithmetic, on
the corpus carried to 11, agrees with them). A project deleted later takes
its golden with it.

## Who calls it

- **Studio** upgrades a library package on open, before a push, and on zip
  import (`upgrade_to_current`, writing back all three file lists).
- **`lp-cli project upgrade`** does the same to a directory.
- **A firmware update's last stage** upgrades a *board's* stored projects:
  `lpa-update`'s project stage reads each project older than the format the
  board's new engine reads (its hello's `build.projectFormat`), runs
  `upgrade_with_chain(files, engine_format, …)` on the host, and writes the
  result back in one atomic batch per project, startup project first, with
  no click; a board found later with older projects is finished the same way
  on connect (`docs/adr/2026-10-28-firmware-updates-upgrade-the-boards-projects.md`).

`upgrade_to(files, target)` runs the chain to a given format rather than to
this build's: a project already at `target` comes back untouched with an
empty report; a `target` past the chain is `TargetBeyondChain` (update the
host); a project newer than `target` is `NewerThanTarget`.
`upgrade_with_chain` runs a caller's chain (how a test proves a toy v12→v13
step rides the same path). `upgrade_to_current` keeps its classifier guard:
an already-current project is still an error for the library callers.

## Firmware

The device never migrates a project — it refuses an old format and says so
(ADR 2026-07-05, decision 5); a host rewrites its files (above, "Who calls
it"). `scripts/check-upgrade-fw.sh`, wired into
`just check-lint`, asserts this crate is absent from the RV32 firmware
dependency graphs.
