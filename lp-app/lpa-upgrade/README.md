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

## Corpus and goldens

- `tests/corpus/v4/<project>/` — real format-4 projects. The two frozen
  `schemas/history/v4/fixtures/` snapshots, with the GLSL and SVG that
  snapshot dropped recovered from `f9d6981dc^`, plus four gallery examples
  recovered whole from the same commit.
- `tests/corpus/v4/_expected/<project>/` — what this crate produces. These
  are **our** contract, human-reviewed once and frozen thereafter. They
  deliberately do not match today's `examples/` and `projects/test/`, which
  were hand-polished past behavior preservation.

Regenerate after an intentional change:

```bash
LPA_UPGRADE_BLESS=1 cargo test -p lpa-upgrade --test corpus_goldens
```

then read every line of `git diff` before committing. A blessed golden nobody
read is a regression test for the bug.

Two tests keep the goldens honest: every migrated project must load through
the real `ProjectRegistry`, and every *unmigrated* one must fail to (a golden
that was never broken proves nothing).

## Firmware

The device never migrates a project — it refuses an old format and says so
(ADR 2026-07-05, decision 5). `scripts/check-upgrade-fw.sh`, wired into
`just check-lint`, asserts this crate is absent from the RV32 firmware
dependency graphs.
