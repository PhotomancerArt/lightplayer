---
status: carried
since: 2026-10-20
logged: 2026-10-20
area: scripts/heap-budget-check.sh (engine arm, steady-render) × PowerButtonNode on fw-emu
related:
  - docs/heap-budget-gate.md
  - lp2025/2026-10-20-0001-playlists-on-the-bus (PR #1215)
---
# The steady-render heap window can catch a PowerButton's open retry

**Shape** — on fw-emu a `PowerButton` (`button:local:D0`, the PLAYFUL
projects' soft power off) fails to open its button and retries on a backoff
(`OPEN_RETRY_INITIAL_MS` 500 doubling to `OPEN_RETRY_MAX_MS` 4000,
`power_button_node.rs`). Each retry costs about 1,280 allocations
(`HardwareSystem::open_button_by_spec` → the virtual button driver's
endpoint stream). The steady-render window captures four frames after two
warm-up frames, so whether a retry lands inside it depends only on how long
the project took to load. Any change that moves load time by a frame can
flip the tryout's `frame.alloc_count` between 284 and ~1,830 with no
per-frame cost changing.

**Carrying cost** — a false "per-frame allocation regression" on an
unrelated change; the honest answers are to work around it or to bake the
retry into the ratchet (which then hides a real regression of that size).

**Workarounds** — check the worst frame before re-baselining: count
allocations per `frame` opening in the profile's `heap-trace.jsonl` and
symbolize the spike (`addr2line -f -C -e target/riscv32imac-unknown-none-elf/release-emu/fw-emu`).
If it is `PowerButtonNode::tick_power_button` → `open_button_by_spec`, it is
this; do not re-baseline the steady-render figures for it.

**Incident log**
- 2026-10-20 — PR #1215: removing the tryout's leftover per-entry durations
  moved a retry into the window (284 → 1,827). The durations were kept
  (inert since a picked entry holds) and only the startup figures were
  re-baselined.

**Exit criteria** — the steady-render window excludes the open retry (the
virtual driver opens `D0`, the retry is moved out of the frame, or the
window is chosen by the trace rather than by frame count), and removing the
tryout's durations moves no steady-render figure.
