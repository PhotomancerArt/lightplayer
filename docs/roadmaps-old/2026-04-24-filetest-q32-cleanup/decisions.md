# Filetest q32 Cleanup — Decisions

#### Mark before fixing

- **Decision:** First establish an annotation baseline with
  `@unsupported` and `@broken` before implementing fixes.
- **Why:** A green baseline makes filetests useful during longer repair
  milestones, and unexpected-pass output catches stale markers.
- **Rejected alternatives:** Fix-only backlog cleanup (keeps suite noisy
  for too long); leave bare failures in CI (weak regression signal).

#### Unsupported means outside q32 by design

- **Decision:** Use `@unsupported` only for behavior that q32 does not
  intend to support, usually across all q32 targets.
- **Why:** No-real-f32 behavior such as NaN/Inf propagation, bit
  reinterpretation, f16/f64 packing, and real IEEE domains is not a
  backend bug.
- **Rejected alternatives:** Treating these as `@broken` (implies they
  should pass on q32); leaving them `@unimplemented` (keeps churn in
  implementation-gap reporting).
- **Revisit when:** A real-f32 or hybrid numeric mode becomes a product
  goal.

#### Broken means intended q32 behavior

- **Decision:** Use `@broken` for current failures that should pass under
  q32 once bugs, harness gaps, or wrong expectations are fixed.
- **Why:** It distinguishes "known bug" from "not in product" while
  preserving unexpected-pass alerts.
- **Rejected alternatives:** `@unimplemented` for all failures (loses
  bug vs missing-feature distinction); no marker (keeps the suite red).

#### q32 semantics require reconciliation

- **Decision:** Numeric fixes target intended q32 semantics using
  `docs/design/q32.md`, the reference `Q32` implementation, and product
  backend behavior together.
- **Why:** q32 is project-defined and has no external standard; the doc
  is the starting source of truth but may lag small implementation
  fixes.
- **Rejected alternatives:** Copy rv32 blindly (rv32 could contain a
  bug); copy the doc blindly (doc could be stale).
- **Revisit when:** q32 gains an externally mandated spec or a formally
  generated conformance suite.

#### Quick expectation and harness fixes go early

- **Decision:** Put suspected wrong expectations, printer mismatches,
  harness parsing gaps, and small q32 numeric edges into M2.
- **Why:** These are low-risk noise reducers and make later subsystem
  milestones focus on real implementation changes.
- **Rejected alternatives:** Leave them with matrix/integer/frontend
  milestones (keeps avoidable noise around longer work).

#### `global-future` is not q32 cleanup

- **Decision:** Keep `global-future/*` out of the broken-fix milestones.
- **Why:** Those tests describe future global `buffer` / `shared` /
  `in` / `out` product surface, not remaining q32 semantics bugs.
- **Rejected alternatives:** Folding them into frontend/memory repair
  milestones (scope creep); marking them as generic q32 bugs (wrong
  product boundary).
- **Revisit when:** The product explicitly adds those global storage
  classes.

#### Validate every milestone

- **Decision:** Every implementation milestone ends with
  `just test-filetests`; targeted `scripts/glsl-filetests.sh` runs are
  for development.
- **Why:** M8 should reconcile, not discover broad regressions for the
  first time.
- **Rejected alternatives:** Only full-sweep at final cleanup (hard to
  bisect regressions); only targeted tests (misses cross-category marker
  drift).
