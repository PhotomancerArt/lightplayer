# ADR: Compile-path allocation discipline

- **Status:** Accepted
- **Date:** 2026-08-07
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The GLSL compiler runs on-device, between render frames, against a 320 KB
heap that the compile transient shares with full per-LED residency (see the
2026-08-03 memory-pressure ADR for the OOM shape this produces at dome
scale). Two reviews established where the cost lives: the 2026-07-05
embedded compiler review (P0 backend fixes landed as PR #57: −41% on-device
compile time) and the 2026-08-07 memory audit
(`Planning/lp2025/_reports/2026-08-07-lps-glsl-lpvm-native-memory-audit.md`),
which found the July frontend "cheap wins" untouched, the Lower stage as the
dominant churn source, and ~50–90 KB of avoidable peak residency in the
backend. Allocator + copy traffic was the largest single self-time category
in the compile window — ahead of any compiler algorithm.

The 2026-08-07 churn pass (PR #390) implemented these findings. This ADR
records the rules the pass established, the evidence protocol that judged
it, and what was deliberately not done.

## Decision

The compile path (lps-glsl, lpvm-native, lp-shader's compile job) holds to
these rules, each grounded in a defect this pass fixed:

1. **Slice, don't refilter.** Span-sorted streams (the token tape) are
   subsliced by `partition_point`, never filter-copied per consumer.
   Corollary from the bug this introduced then fixed (163a76318): the
   trailing Eof token sits *at* source end, not past it — and lps-glsl
   parser changes must be validated with `cargo test -p lp-shader`, whose
   inline no-trailing-newline sources catch what the newline-terminated
   filetest corpus cannot.
2. **Parse once.** An initializer's `ParsedExpr` is carried to its consumer,
   not re-parsed per stage. The retention this trades (trees held into
   typeck) is measured and accepted; drop-after-consume is the recorded
   lever if the frontend peak becomes the binding constraint.
3. **No borrowck-dodge clones on per-node paths.** Hot loops borrow-split
   the context (`let Ctx { fb, locals, .. } = ctx`) or take free functions
   over disjoint fields. A `.cloned()` whose only justification is the
   borrow checker is a defect on a per-expression path.
4. **Inline the common cardinality.** Per-value element lists use
   `InlineVec` (`lps-glsl/src/small.rs` — hand-rolled, no deps, no unsafe):
   4 lanes / 16 IR types inline, heap only for wide shapes. New external
   dependencies are not the answer to allocation churn in no_std crates.
5. **Probe paths must not allocate.** Speculative checks (`is_constructor_
   name`, place prechecks, registry membership) answer from non-allocating
   lookups; `format!` diagnostics are constructed only on paths that
   actually report. A discarded `Result<_, Diagnostic>` is a defect.
6. **Emit by draining.** Code emitters free each encoded run as it is
   copied into the final image (xt `finish()`), and linking `mem::take`s
   each function's code rather than concatenating alongside it. Peak is
   sized by what is *simultaneously* alive, not by what is ever allocated.
7. **Intermediates die at their last reader under device config.** With
   `debug_info` off, a function's LPIR body is released when Lower consumes
   it, `alloc_output` is never materialized, and lowered state drops at end
   of Emit. Debug builds keep everything; the device pays only for what the
   device reads.
8. **Exactness over silence in fixed-size sets.** `RegSet` gained an
   overflow tail because its silent ≥256 drop produced wrong codegen (a
   loop-carried vreg lost its increment — device hang, found and fixed as
   264d4ad6f). A capacity-limited set on a correctness path either errors
   or grows; it never ignores.

**Evidence protocol** (how compile-path perf changes are judged):
- Acceptance: `lp-cli profile <workload> --collect alloc,cpu` per phase, on
  `examples/basic` (largest GLSL) and `examples/zook-dome` (1,500-LED real
  use case), reading the shader-compile window slice and whole-run peak.
  Alloc counts are deterministic; cycle windows jitter ~±3%.
- Safety: the filetest corpus (37,929 cases / 852 files) and all compile-
  path crate tests must pass **unchanged** — emitted bytes are the
  contract. No expectation may be adjusted to absorb a perf change.

## Consequences

Measured P1→final on PR #390 (emu, esp32c6 model): compile-window allocs
−30.6% (basic) / −27.6% (zook-dome), window bytes −15.9% / −12.7%, compile
cycles ≈−10% / ≈−5%, zook window transient −15.3%. basic's whole-run peak
moved +3.6 KB *against* us — it sits mid-frontend where rule 2's retention
lands, and the backend releases don't lower a frontend high-water mark; the
backend-window reductions are the ones that matter for the dome-scale OOM
shape, and the xt-emitter/link wins don't appear in rv32 emu numbers at
all. Cumulative with PR #57, the compile window's allocation count is down
~80% from the July baseline (11,100 → 2,253).

**Explicitly deferred, with re-measure triggers:**
- **Identifier/type interning** (`&'src str` AST, `TypeId`; ~3× working-set
  cut per the July estimate) — the remaining structural lever; re-profile
  after this pass before committing to it.
- **Budgeted ShaderNode compile driver** — the dropped-frames fix is
  scheduling, not memory: `compile_px_desc` still drives the fully-budgeted
  job to completion inside one render tick. Separate plan.
- **Backend meta copy** (`LpsModuleSig` ×2 through the backend window) —
  contained follow-up: move meta into `NativeCompileJob` + `take_sig()`.
- **rv32 12 B/VInst emit reservation** — deliberate anti-fragmentation;
  re-measure the density before touching.
- **Regalloc walk time-only patterns** — carried as
  `docs/debt/regalloc-walk-superlinear-passes.md`.

Related: `docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`
(the seam this pass shrinks the transient for), PR #57 (July P0 batch),
plan `lp2025/2026-08-07-2103-compiler-memory-churn-pass` (per-phase
evidence in `evidence.md`).
