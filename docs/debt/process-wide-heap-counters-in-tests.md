---
status: paying-down
since: 2026-08-28
logged: 2026-09-25
area: host heap-measurement tests (lpc-engine/tests, lpvm-native/tests)
related:
  - PR #827 (multi-pattern projects)
  - lp-core/lpc-engine/src/test_alloc_counter.rs (the thread-local pattern)
---
# Heap-measurement tests count every thread in the process

**Shape** — Several host tests answer "how much heap does this retain /
peak at?" with a `#[global_allocator]` that bumps process-wide atomics,
and guard it with "one `#[test]` per binary". That guard is not enough:
the libtest harness is still running other threads, and the counter sees
them. The one that bit is the harness's own main thread. In the parallel
runner (`library/test/src/lib.rs`, `run_tests`, any `--test-threads` > 1,
which is the default on a multi-core runner) it spawns the test thread and
only *then* allocates its `running_tests` map and `timeout_queue` entry
(`running_tests.insert` / `timeout_queue.push_back` right after
`run_test`), then blocks on the result channel. On a loaded runner that
lands inside the test's measurement window. Other sources are the
slow-test (>60 s) warning, and helper threads the code under test runs:
the host graphics backend is wasmtime with `parallel-compilation` on by
default, so shader compiles allocate on rayon's pool threads.

Because the window is a race, the test passes on the desk and fails on a
loaded CI runner with a figure that is not the product's.

**Carrying cost** — red CI on a PR that did not change the measured code,
and a false "leak" to chase. The instinct it invites (loosen the
threshold, add a retry) would blind the test to the leak it exists to
catch.

**Workarounds** — count per thread. Give the allocator `thread_local!`
`const`-initialised `Cell` counters (no destructor, so touching them from
inside the allocator never allocates), make the live count signed (a
thread may free what another allocated), and use `try_with` so a thread
tearing down is not a panic. `lpc-engine/src/test_alloc_counter.rs` and
`tests/playlist_cycle.rs`'s `alloc_count` already worked this way; the
three counters below were converted to it. A test whose claim genuinely
spans threads needs its threads named and joined inside the window, not a
process-wide counter.

**Incident log**

- 2026-09-25 — PR #827, CI run 36200021959, Validate (x64):
  `node_tree_reload_memory` `reloading_a_subtree_retains_no_heap` failed
  "100 reloads grew retained heap by 900 B" (warm 5358 B → 6258 B) at a
  head whose only change from the last green one was a docs-only merge
  from main. 900 B is about the size of the harness's first `running_tests`
  table plus a four-slot `VecDeque<TimeoutEntry>`. The same PR's
  `playlist_crossfade_memory` also flaked locally under load ("x10:
  active→idle never sized its fade scratch"). Fixed the same day by
  per-thread counting in `node_tree_reload_memory.rs`,
  `entry_residency_memory.rs` and `playlist_crossfade_memory.rs`,
  assertions unchanged. Evidence on an M2 Max: 200 / 50 / 30 sequential
  runs, then 2000 / 400 / 200 runs 48 / 32 / 32 at a time (load average up
  to 38), with no failures.
- 2026-09-26 — auto-queue ticket `2026-09-26-heap-counters-engine-tests`:
  converted the four remaining `lpc-engine/tests` probes
  (`per_lamp_memory_table.rs`, `per_node_memory_table.rs`,
  `project_read_peak_memory.rs`, `zook_load_tick_memory.rs`) to the same
  `thread_local!` `const` `Cell<isize>` pattern, `try_with` throughout;
  every assertion and threshold left unchanged. `project_read_peak_memory.rs`
  kept its `MEASURE_LOCK` mutex (no longer needed for correctness once the
  counters are per-thread, but harmless, and removing it was out of this
  ticket's scope). See the PR for per-test pass evidence.

**Exit criteria** — no host heap test counts through a process-wide
counter. Still process-wide on 2026-09-26:
`lp-shader/lpvm-native/tests/support/peak_alloc.rs`. Retire when that is
converted, or shown to measure only across threads it owns.
