# Phase 12 — Final cleanup, workspace validation, summary

> Read [`00-notes.md`](./00-notes.md) and [`00-design.md`](./00-design.md)
> before starting.
>
> **Depends on:** every prior phase (01–11) merged. This is the
> last gate before declaring M3 done.
>
> Solo phase. Cannot parallelize with anything else.

## Scope of phase

Final consolidation pass:

1. **Run the full local CI** (`just ci` equivalent) and fix any
   warnings, drift, or mistakes that earlier phases introduced.
2. **Sweep for stragglers**: stale TODOs that earlier phases
   were supposed to resolve, dead code, orphaned test fixtures,
   stale `mod` declarations, etc.
3. **Verify all M3 acceptance criteria** from the milestone are
   met, with one-line evidence per criterion.
4. **Write the milestone summary** at
   `docs/plans/2026-04-22-lp-domain-m3-visual-artifact-types/summary.md`,
   following the predecessor milestone's summary shape
   (`docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md`).

This phase **does not** introduce new code or examples. If
something is missing, *stop and surface the gap* rather than
patching it under the cleanup heading.

**Out of scope:**

- New features, new examples, new types.
- Anything mentioned as deferred in earlier phases (audio
  Kinds beyond `AudioLevel`, `LiveSelection`, per-entry
  Playlist transitions, schema codegen tooling, migration
  framework, cross-artifact resolution).
- Touching `docs/plans-old/`.
- Moving the plan directory to `docs/plans-old/` — the
  plans-tooling does that, not this phase.

## Conventions

Per [`AGENTS.md`](../../../AGENTS.md):

- All cleanup edits respect the
  "tests at the bottom, types at the top" file layout.
- The summary file mirrors the structure of
  `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md`:
  brief milestone restatement, what shipped, decisions logged,
  deferrals + future work, validation evidence.
- Keep the summary under ~150 lines.

## Sub-agent reminders

- Do **not** commit.
- Do **not** silently fix gaps in earlier phases by adding new
  code here. If a gap exists, stop and report it.
- Do **not** delete or rewrite code from earlier phases except
  to fix lint warnings or remove stale TODO comments that
  earlier phases were supposed to resolve.
- The summary file is **the** deliverable for this phase. Be
  precise; this file is the M3 historical record.
- If the workspace lint baseline is dirty (i.e. there were
  warnings in `lp-domain` *before* M3), do not chase pre-
  existing warnings. Note them in the summary's
  "Known dirt" subsection and move on.
- Run validation on a clean tree (no untracked junk in
  `lp-domain/lp-domain/`).
- If something blocks, stop and report back.
- Report back: validation command output (success / failure
  summary, NOT full transcripts), summary.md path, list of
  cleanup edits made.

## Cleanup checklist

Walk through and resolve each item. Items 1–6 are blockers; 7–10
are nice-to-have if time permits.

1. **`TODO(M3)` strings.** `rg 'TODO\(M3\)' lp-domain/` should
   turn up zero hits. The `binding.rs` `TODO(M3)` from M2 must be
   resolved by Phase 01. Any others mean an earlier phase missed
   work.

2. **Stale `mod` declarations.** `lp-domain/lp-domain/src/lib.rs`
   should re-export `visual` (Phases 06+07) and `artifact::load`
   (Phase 08). No `pub mod` left commented-out or behind a
   `#[cfg(any())]` placeholder.

3. **Cargo.toml hygiene.** New deps from phases 02–10 (`toml`,
   `lpfs`, `serde_json`, `jsonschema`) all carry sensible
   feature flags. No accidental default-features-on for crates
   that should be `default-features = false`. The
   `lp-domain/lp-domain/Cargo.toml` `[features]` block lists
   `std` and `schema-gen` and they each gate the right
   surface (`std` gates `lpfs::LpFsStd`-using paths;
   `schema-gen` gates the `JsonSchema` derives + the schema
   smoke tests + the schema-drift integration tests).

4. **Examples actually load.** Run the round-trip integration
   tests and confirm every example file in
   `lp-domain/lp-domain/examples/v1/` is exercised by at least
   one `#[test]` (no orphan example files).

5. **Old `docs/design/lpfx/{patterns,...}/` files are gone.**
   `git status` should show them as `D` (deleted) and the new
   `lp-domain/lp-domain/examples/v1/` files as `A` (added or
   `??` if pre-commit). No leftover empty subdirs in
   `docs/design/lpfx/`.

6. **Schema-gen smoke tests still pass.** The `schema_gen_smoke`
   tests added in M2 should now also include the new Visual
   types. If they don't, add them — derive-driven `schema_for!`
   calls are one-liners.

7. **Doc-link sweep**: `rg 'design/lpfx/(patterns|effects|
   transitions|stacks|lives|playlists)/' docs/design/` returns
   zero hits (Phase 11 should have handled this; verify).

8. **Rustdoc warnings**: `cargo doc -p lp-domain --no-deps
   --features schema-gen 2>&1 | rg -i warning` returns zero
   hits.

9. **Justfile entry**: if the `justfile` already has a
   `test-lp-domain` or similar recipe, confirm it still passes.
   No new recipe required for M3.

10. **`AGENTS.md` doesn't need an update** for M3 — its scope is
    the GLSL JIT pipeline, which is orthogonal to the domain
    crate. Confirm and move on.

## Validation

Run, in order, on a clean tree:

```bash
# Format + clippy (workspace-wide via the just recipes that
# AGENTS.md sanctions). Either should be a no-op for lp-domain
# since no shader-pipeline crates change.
just check

# Targeted lp-domain validation:
cargo check -p lp-domain
cargo check -p lp-domain --features std
cargo check -p lp-domain --features schema-gen
cargo check -p lp-domain --features std,schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features std
cargo test  -p lp-domain --features schema-gen
cargo test  -p lp-domain --features std,schema-gen
cargo test  -p lp-domain --test round_trip
cargo test  -p lp-domain --test round_trip --features std,schema-gen

# Rustdoc warnings:
cargo doc -p lp-domain --no-deps --features schema-gen

# Stale-pattern sweep:
rg 'TODO\(M3\)' lp-domain/
rg 'design/lpfx/(patterns|effects|transitions|stacks|lives|playlists)/' docs/design/
```

All `cargo` commands: zero warnings, zero failures. Both `rg`
sweeps: zero hits.

The full `just ci` is overkill for an `lp-domain`-only milestone
(it builds firmware images), but if there's any doubt that the
domain crate change touched something beyond its boundary, run
`just ci` once at the end. If the user requests CI parity, it's
the right call.

## Acceptance-criteria evidence

For the M3 summary, restate each milestone bullet from
`docs/roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md`
with one line of evidence (file path + Phase number, or test
name) showing it was met. Skeleton:

| Milestone bullet                            | Met by                                                  |
| ------------------------------------------- | ------------------------------------------------------- |
| Six typed Visual structs (`Artifact`)       | `lp-domain/lp-domain/src/visual/{pattern,effect,...}.rs` (P06+P07) |
| `ShaderRef` / `[bindings]` / `[input]`      | `visual/shader_ref.rs`, `visual/visual_input.rs` (P04); `Live`/`Playlist` carry `bindings` (P07) |
| `Slot` TOML grammar                         | `shape.rs` (P03), `value_spec.rs` Kind-aware default parse (P05) |
| New signal Kinds                            | `Kind::AudioLevel` in `kind.rs` (P02)                   |
| Eight canonical examples in `examples/v1/`  | `lp-domain/lp-domain/examples/v1/` (P09)                |
| Round-trip integration tests                | `lp-domain/lp-domain/tests/round_trip.rs` (P10)         |
| `LpFs`-based loader stub                    | `lp-domain/lp-domain/src/artifact/load.rs` (P08)        |
| Updated design docs / deleted old TOMLs     | (P11) + `git status` deletions (P09)                    |

Drop any bullet that the milestone document doesn't list and add
any extra evidence the actual implementation produced.

## Summary file structure

`docs/plans/2026-04-22-lp-domain-m3-visual-artifact-types/summary.md`:

```markdown
# M3 — Visual artifact types + canonical examples + TOML grammar — summary

Roadmap: [`docs/roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md`](../../roadmaps/2026-04-22-lp-domain/m3-visual-artifact-types.md)
Plan dir: this directory.

## What shipped

- (one bullet per Phase 01–11, in execution order, ~1 line each.)

## Decisions logged

- (the resolved Q-D1…Q-D7 + Q1…Q15 from `00-notes.md`,
  one line each. Link to the notes file for context.)

## Examples corpus (`lp-domain/lp-domain/examples/v1/`)

- (list every example file shipped, one line each.)

## Deferred to later milestones

- (LiveSelection, per-entry Playlist transitions,
  cross-artifact resolution, schema codegen tooling,
  migration framework, audio Kinds beyond AudioLevel,
  Q32 specialization. Each line names which milestone
  picks it up if known.)

## Validation evidence

- (paste one-line-per-command success summaries from the
  validation block above — NOT full output. Mention
  test counts.)

## Known dirt

- (anything that's still scrappy but acceptable for v1:
  e.g. binding-key strings unparsed, params overrides as
  raw `toml::Value`, Live as a placeholder. Each line
  links to where it gets cleaned up.)

## Files touched

- (compact list grouped by phase: P01 → `binding.rs`,
  P02 → `kind.rs`, etc. Skip if the changeset is
  large; just point at `git diff main...HEAD --stat`.)
```

Mirror this skeleton; don't reinvent the section names.

## Definition of done

- All blocker cleanup-checklist items (1–6) resolved.
- All validation commands pass with zero warnings.
- `summary.md` written to the plan directory matching the
  skeleton above, ≤150 lines, accurate.
- M3 acceptance-criteria evidence table complete.
- No new features, no new examples, no new types beyond what
  Phases 01–11 produced.
- No commit.

Report back with: cleanup-checklist items completed (numbered),
validation command pass/fail summary, summary.md path, and any
deviations or open issues that should block declaring M3 done.
