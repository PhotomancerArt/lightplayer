---
status: carried
since: 2026-08-01
logged: 2026-08-01
area: lps-probe/tests
related: ["story-capture-pipeline.md (same machine-load class)"]
---
# lps-probe wall-clock perf test fails under ambient machine load

**Shape** — `experiment::tests::perf_4096_render_evals_under_10s_debug`
asserts a 10 s wall-clock bound and runs in the default suite
(`test-rust-core`). Clean-machine runtime is ~8 s (debug), so the bound
carries only ~20% headroom — and the test has no isolation from ambient
load. This is structural because the documented review-gate workflow
*requires* competing load: `docs/process/review-gates.md` says the studio
dev server must be running at every visual-gate handoff, and a `dx` watch
build plus a second agent worktree reliably eats the margin. The perf
number exists to decide whether probe evaluation needs a worker offload —
a real question — but a hard assert in the default suite converts "the
machine was busy" into a red gate.

**Carrying cost** — `just test` / `just check test` fails spuriously
whenever a dev server or parallel agent session is active (three times on
2026-08-01 alone: 10.02 s, 18.01 s, 18.33 s vs the 10 s bound); each
failure costs a diagnose-and-rerun cycle, and the failure lands in an
unrelated PR's validation, casting doubt on good changes.

**Workarounds** — Stop the studio dev server (and pause heavy sibling
sessions) before running the full gate; rerun `cargo test -p lps-probe
--lib` in isolation to confirm (passes ~8 s clean). Do NOT widen the
bound to make churn stop without deciding what the number is for
(story-capture lore: thresholds widened under pressure stop measuring).
Also: capture full logs when backgrounding the gate — `… | tail`
reports tail's exit status and has masked this failure twice (same trap
already recorded in story-capture-pipeline.md).

**Incident log**
- 2026-08-01 — three consecutive full-gate failures during the M3
  boards-catalog visual-gate session while the worktree dev server and a
  second agent session were active; passed clean between them with the
  server stopped. Filed this entry.

- 2026-08-02 — M5 provisioning-gate session: `perf_4096_render_evals_under_10s_debug`
  took **10.998 s** against the 10 s bound with the worktree dev server and a
  story-PNG capture running; the same test passed at **7.86 s** in isolation
  moments later. Third sighting of the same shape; no code in the failing
  crate had changed.

- 2026-08-02 (second, M5 merge gate) — 10.49 s under a concurrent story
  capture; 4.04 s isolated minutes later. **This one masked a real
  failure**: `test-rust-core` aborting means `just test` never reaches
  `test-studio-host`, so a genuine compile break in `lpa-studio-web`
  (a stale test call after a signature change) shipped to CI unseen and
  failed there instead. The flake's cost is no longer just a re-run —
  it hides the recipes behind it. Workaround while it stands: when
  `test-rust-core` fails on this test alone, re-run the LATER recipes
  explicitly (`just test-studio-host`) before believing the tree.

**Exit criteria** — The default suite contains no load-sensitive
wall-clock assert: the perf measurement either moves behind an opt-in
feature/recipe (perf job), switches to a load-insensitive proxy (eval
count / instruction budget), or gains enough isolation that a dev server
plus a sibling build cannot fail it. The measurement itself must survive
— it gates the P5 worker-offload decision.
