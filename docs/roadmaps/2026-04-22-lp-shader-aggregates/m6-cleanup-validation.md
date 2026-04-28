# Milestone 6 — Cleanup, RV32 sweep, docs

## Goal

Final integration sweep across the roadmap. Ensure every
`@unimplemented` marker that should have been toggled off by M2/M3/M4
actually is; resolve any RV32 backend gaps left as known issues by
earlier milestones; validate domain-workload perf deltas (R4); update
project docs to describe the new aggregate ABI; archive completed plans.

## Suggested plan name

`lp-shader-aggregates-m6-cleanup-validation`

## Scope

### In scope

- **Filetest sweep.** Walk
  `lp-shader/lps-filetests/filetests/{struct,function,uniform,
  array,const}/` and verify every `@unimplemented` marker for backends
  in scope of this roadmap is either:
  - off (because the test passes), or
  - left on with an inline `// TODO(roadmap-followup): <reason>` comment
    referencing a filed follow-up issue or roadmap item.
  No silent skips.
- **RV32 backend sweep.** Run the full filetest suite on `rv32c.q32`
  and `rv32n.q32`. For each remaining failure:
  - If it's a foundational ABI bug surfaced by an unusual test shape,
    fix it here.
  - If it's a known-deferred item (e.g. cross-call sret edge case),
    document and file a follow-up.
- **Domain-workload benchmark.** Re-run the M5 bench suite (R4
  mitigation evidence). If tiny-aggregate hot paths are still
  measurably slower than the pre-roadmap baseline by more than a
  pre-agreed threshold (settle in the M6 phase: e.g. >10%), file a
  follow-up roadmap item for the small-struct register-return fast
  path. Capture results in
  `docs/roadmaps/2026-04-22-lp-shader-aggregates/m6-bench.md`.
- **Documentation updates.**
  - `lp-shader/README.md`: update the "ABI" / "calling convention"
    section to describe the unified pass-by-pointer aggregate ABI,
    `LpvmDataQ32` host-side flow, sret returns. Reference the
    roadmap.
  - Any per-crate README or module doc-comment that previously
    described aggregate ABI (`lpvm/src/lpvm_abi.rs` doc header,
    `lps-frontend/src/lower_call.rs` doc header) brought up to date.
- **Plan archival.** Move the per-milestone plan directories
  (`docs/plans/2026-04-22-lp-shader-aggregates-m1-…`, …
  `lp-shader-aggregates-m5-…`) to `docs/plans-old/` per the standard
  plan-cleanup convention.
- **Decisions file.** Author
  `docs/roadmaps/2026-04-22-lp-shader-aggregates/decisions.md` per the
  roadmap process — short, scannable, retrievable. Capture the
  cross-cutting decisions that span milestones (unified pointer ABI,
  always-sret returns, deferred small-struct register-return,
  read-only-in optimisation as separable pass, etc.).

### Out of scope

- Any new aggregate features. M6 is sweep + docs only.
- Performance optimisations beyond verifying M5's gains stuck.
- Small-struct register-return fast path (follow-up roadmap if R4
  benchmark says it's needed).

## Key decisions

- No new design decisions. Any decisions surfaced during sweep are
  scoped to the underlying milestone and either resolved in-line or
  filed as follow-ups.
- **Bench-regression threshold for "ship vs follow-up roadmap."**
  Settled during the M6 work; suggested 10% for tiny-aggregate hot
  paths and 5% for `examples/basic` end-to-end frame time.

## Deliverables

- Filetest corpus with all in-scope `@unimplemented` markers either
  off or annotated with TODO + follow-up reference.
- Bench artefact:
  `docs/roadmaps/2026-04-22-lp-shader-aggregates/m6-bench.md`.
- Updated docs:
  - `lp-shader/README.md`
  - any affected module doc-comments
- New file:
  - `docs/roadmaps/2026-04-22-lp-shader-aggregates/decisions.md`
- Archived plan directories under `docs/plans-old/`.

## Dependencies

- **Requires M1–M5 complete.** M6 cannot start until the substantive
  work is done.

## Execution strategy

**Option A — Direct execution (no plan file).**

Justification: M6 is a mechanical sweep + docs. No design questions,
no architecture decisions, no new features. A Composer 2 sub-agent can
work straight from this milestone file with no further planning, then
hand back the bench results and the decisions file for review.

**Suggested chat opener:**

> I can implement this milestone without planning. Here is a summary of
> decisions/questions:
>
> - Sweep `@unimplemented` markers across the whole struct/array
>   filetest corpus; toggle off where the test passes, annotate with
>   TODO + follow-up reference where it doesn't.
> - Run RV32 (`rv32c.q32`, `rv32n.q32`) end-to-end and resolve or file
>   any remaining gaps.
> - Re-run M5 bench suite; if tiny-aggregate regression >10% or
>   `examples/basic` regression >5%, file a small-struct register-return
>   follow-up roadmap.
> - Update `lp-shader/README.md` aggregate-ABI section and any affected
>   module docs.
> - Author `decisions.md` per roadmap process.
> - Move per-milestone plan dirs to `docs/plans-old/`.
>
> If you agree, I will implement now using a Composer 2 sub-agent. If
> you want to discuss any of these, let me know now.
