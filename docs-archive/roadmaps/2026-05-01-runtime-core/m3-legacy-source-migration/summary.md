### What was built

- Legacy node configs moved wholesale from `node.json` to `node.toml` across loaders, builders, templates, tests, and examples, with `lpc-source::legacy` owning loader policy and `lpfs` implementing generic read/discovery traits (no `lpc-source` → `lpfs` dependency cycle).

### Decisions for future reference

#### Wholesale `node.toml` switch (no long-term JSON loader)

- **Decision:** Live sentinel is `node.toml` only; there is no retained compatibility path that keeps loading `node.json` in runtime or builders.
- **Why:** Finishes the milestone with one authoritative on-disk shape and avoids carrying dual loaders indefinitely.
- **Rejected alternatives:** Temporary dual JSON+TOML support — rejected because it prolongs migration debt and divergent test/fixture expectations.
- **Revisit when:** If external users still ship `node.json`-only trees, introduce an explicit one-shot migration tool rather than runtime fallback.

#### Source-owned traits; `lpfs` implements them

- **Decision:** Node discovery and typed TOML load policy live in `lpc-source`; filesystem integration stays in `lpfs` behind traits (same pattern as artifact reads).
- **Why:** Avoids a crate cycle (`lpfs` already depends on `lpc-source`) while keeping loading rules centralized for non-engine callers.
- **Rejected alternatives:** `lpc-source` depending on `lpfs` — rejected due to cycle; engine-only loading — rejected because source loading is useful outside the engine.
- **Revisit when:** If a third filesystem backend appears, extend trait implementations rather than moving concrete `LpFs` calls into `lpc-source`.

#### Examples are first-class migration artifacts

- **Decision:** All shipped examples were converted to `node.toml` alongside code changes so checked-in trees stay loadable.
- **Why:** Examples are the primary offline reference for project layout; leaving JSON would make docs and tooling disagree with the loader.
- **Rejected alternatives:** Examples-only JSON shim — rejected as inconsistent with wholesale switch.
- **Revisit when:** N/A for format; add more perf examples if new workloads need coverage.

#### Runtime validation vs profiler finalization

- **Decision:** Accept demo/device validation for the source-format migration and treat the `lp-cli profile` teardown stall as a separate profiler issue.
- **Why:** The converted source loaded and rendered in the real demo/device paths; the profiler reached project load/JIT/frame driving before stalling during finalization.
- **Rejected alternatives:** Blocking M3 on profiler teardown — rejected because it is outside the source loader behavior being migrated.
- **Revisit when:** The profiler finalization path is debugged or a smaller reliable profile smoke command is added.

#### Doc vs live references to `node.json`

- **Decision:** Roadmaps, archived plans, and migration notes may still mention `node.json` historically; live Rust comments, doctest paths, and filesystem tests should prefer `node.toml` where they describe current behavior.
- **Why:** Preserves accurate historical discussion without misleading readers of API docs or generic FS tests.
- **Rejected alternatives:** Global repo-wide scrub of every `node.json` string — rejected as scope creep into archived material per milestone notes.
- **Revisit when:** If confusion persists, add a short glossary in developer docs (outside this milestone).
