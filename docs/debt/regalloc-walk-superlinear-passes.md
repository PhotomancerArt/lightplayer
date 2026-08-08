---
status: carried
since: 2026-06 # the regalloc walk's original shape
logged: 2026-08-07
area: lpvm-native regalloc walk
related:
  - "plan: ~/.photomancer/planning/lp2025/2026-08-07-2103-compiler-memory-churn-pass/ (P5 explicitly skipped these)"
  - "report: Planning/lp2025/_reports/2026-08-07-lps-glsl-lpvm-native-memory-audit.md (backend findings #7, #8)"
---
# The regalloc walk carries two super-linear passes that are fine at today's scale

**Shape** — two compile-time (not memory) patterns in
`lp-shader/lpvm-native/src/regalloc/walk.rs`, deliberately left in place by
the 2026-08-07 memory-churn pass because they are time-only, riskier to
restructure than the allocation work, and invisible at current shader sizes:

- **Per-loop liveness re-walk.** Every `Region::Loop` runs `analyze_liveness`
  plus two `defs_in_region` scans over its full subtree, so nested loops
  re-scan inner bodies once per nesting level — O(N × depth) instruction
  visits. No heap involved (`RegSet` is stack-first). A single bottom-up pass
  caching per-region results would fix it.
- **Insertion sort on the edit list.** The boundary/loop reload edits are
  pushed out of anchor order and repaired by insertion sort — O(inversions),
  with displacement bounded by region size. Deliberate at the time (avoids
  sort scratch allocation); with heavy spilling (1–3 K edits) worst case is
  noticeable but bounded.

**Why carried** — at ~2,000 VInsts and shallow loop nesting both are noise
next to the emit and liveness constants. They become real when dome-scale
shaders push instruction counts and loop depth up together.

**Trigger to retire** — a `--collect cpu` profile attributing a visible
share of the compile window to `walk_linear_range`/`analyze_liveness` at a
real workload, or shader corpus growth pushing typical VInst counts past
~10 K.
