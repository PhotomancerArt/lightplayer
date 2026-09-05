---
status: open
found: 2026-09-04
area: lpa-server ProjectRead headroom gate vs the classic ESP32's fragmented two-region heap
class: stand-in-divergence   # largest-free-block stands in for "can this read afford to run"
related:
  - 2026-08-26-project-read-assembly-oom-resets-classic.md
  - 2026-08-29-shader-jit-compile-transient-starves-classic-heap.md
  - 2026-08-29-load-project-resets-instead-of-refusing.md
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
  - ../reports/2026-09-04-classic-heap-fragmentation.md
  - ../heap-budget-gate.md
---
# The read gate refuses on a largest-block proxy while the read needs volume — and names a remedy the client already applied

**Shape** — opening a project in Studio against the desk classic fails,
terminally:

```
protocol error: project read failed: read refused: heap headroom too low
(largest free block 25511 B < 32768 B); narrow the query
(include_slots:false, one probe per read, or page nodes by id) and retry
```

The board is healthy, the connection stays up, the engine keeps ticking.
Studio simply never syncs, and the error tells the user to do three things
Studio is already doing.

**Reproduced on silicon (2026-09-05, desk classic DOM-Z-102, first-fit
build from the same branch)** — with the stamped startup project
`/projects/studio` resident, the board rests at 34,160–34,288 B free and a
25,509–25,536 B largest block from its first compiled frame on, and every
one of Studio's staged reads is refused: the skeleton, the slot pages and
the single probe alike. Studio connects into this state; the failure is the
board's normal resting state, not a race. Capture and log:
`bench/bench-llff-reload.csv` in the planning directory
`lp2025/2026-09-04-1358-classic-heap-fragmentation-research`.

**Mechanism** — two mismatches, one on each side of the sentence.

*The gate measures contiguity; the read needs volume.*
`PROJECT_READ_MIN_HEADROOM_BYTES` (32 KiB, `lp-app/lpa-server/src/server.rs`)
gates on the embedder's `largest_free_block` probe. Its doc comment records
one calibration point — the 2026-08-29 G1 bench walk, where the gate at
16 KiB admitted a read that then OOM-reset the board, so the number was
doubled to "one frame budget × 2". Measured on the emulator at device width
(`--workload studio-sync`, the `project-read` window, tree `06946a2ea`), the
read it guards has:

| | zook-dome | basic | meteor |
|---|---:|---:|---:|
| transient | 23,723 B | 23,723 B | 23,723 B |
| retained | 0 B | 0 B | 0 B |
| **largest single allocation** | **4,096 B** | **4,096 B** | **4,096 B** |
| allocations in the opening | 599 | 599 | 609 |

The gate demands one contiguous 32,768 B block before beginning a read whose
largest single ask is 4,096 B — eight times smaller — and whose whole
working set is 23,723 B spread over ~600 allocations, none of which it keeps.

*The remedy is a dead end.* "Narrow the query (include_slots:false, one probe
per read, or page nodes by id)" lists the three things
`lp-app/lpa-studio-core/src/app/project/project_sync.rs` already does, by
ADR `2026-08-28-project-reads-bounded-streamed-refusable` rule 3:
`initial_skeleton_read_request` sends `include_slots: false` with no probes,
`initial_slot_page_requests` pages node detail `INITIAL_SYNC_SLOT_PAGE_NODES
= 16` ids at a time, `initial_probe_read_requests` sends one probe per read.
The monolithic read is no longer constructible. A client that follows the
advice literally has nowhere left to go.

**Evidence** — `docs/reports/2026-09-04-classic-heap-fragmentation.md`,
section 5. Replaying the recorded allocation trace on the classic's
two-region first-fit layout (110 KiB arena + 72 KiB SRAM1 tail, filled in
registration order) shows markers where the heap has ample total free and
would still refuse:

| project | marker | largest free | total free | read needs | verdict at the 32 KiB gate |
|---|---|---:|---:|---:|---|
| `examples/basic` | `shader-compile E` | 25,168 B | 57,128 B | 23,723 B | refused, 7,600 B short of the proxy |
| `examples/basic` | `frame E` (last) | 25,168 B | 57,312 B | 23,723 B | refused |
| `examples/meteor` | `shader-compile E` | 31,128 B | 50,684 B | 23,723 B | refused, 1,640 B short of the proxy |
| `examples/meteor` | `frame E` (last) | 31,128 B | 50,900 B | 23,723 B | refused |

57 KB free, a 4 KB largest ask, a 24 KB transient — refused. The structural
reason the largest block collapses while total free does not is in the same
report, section 3: `esp_alloc` fills regions in registration order, so by the
close of the first frame the 110 KiB arena's largest block is 168 B (basic) /
36 B (meteor), and the whole-heap largest block is region 1's number from
then on.

The gate is not simply too high. At `examples/basic`'s `shader-link B`
marker (16,980 B largest, 46,872 B free, 70 holes, 38 of them under 64 B) a
23,723 B transient of ~600 allocations *is* genuinely at risk, and refusing
is right. One scalar cannot tell those two situations apart.

**Why it stays refusal-shaped for now** — the alternative the gate exists to
prevent is real and worse: an infallible allocation aborting mid-assembly
RESETS the board
(`docs/defects/2026-08-26-project-read-assembly-oom-resets-classic.md`).
Lowering the constant without changing the allocation posture just trades
refusals for resets. That is why this is filed as a defect in the *shape* of
the check, not as "the number is wrong".

**Fix direction** — make the read path fallible and let the attempt be the
measurement:

- `try_reserve` on the sink's `pending_events` batch
  (`lpc-shared/src/transport/server.rs`) and on the atom builders, so an
  unaffordable read fails with a structured, **retryable** error instead of
  aborting.
- Studio backs off on that error — smaller slot pages, fewer probes per read
  — rather than being told to do what it already did.
- `PROJECT_READ_MIN_HEADROOM_BYTES` retires with the abort path it was
  guarding, rather than being re-tuned against a second bench point.

Ranked as follow-up 2 in `docs/reports/2026-09-04-classic-heap-fragmentation.md`
(the levers ranked above it recover contiguity; this one is what makes the
reported failure stop happening). Its own session — this record is the
statement of the problem, not the design.

**Repro** — open a project in Studio against a classic that has loaded a
project and compiled its shader. Emulator-side, without a board:

```bash
cargo run -p lp-cli -- profile examples/basic --collect alloc --mode startup \
    --workload studio-sync \
    --frag-discount-site VirtualWs281xDriver::endpoints --frag-discount-site HwResource
```

and read the fragmentation section's `largest` against `free` at
`shader-compile E`, and the `project-read` window's budget line.

**Regression coverage** — none yet, and the gap is the point:
`lp-app/lpa-server/tests/project_read_refusal.rs` pins that a low probe
value produces a refusal, which is the *current* behaviour, not the
contract. Nothing anywhere asserts that a heap which can serve the read is
allowed to. The measurement that would have caught it — the `project-read`
window's `transient` and `largest alloc` beside the gate constant — did not
exist until this plan built it.

**Lesson** — a one-scalar affordability gate is a stand-in, and a stand-in
has to be checked in the dimension it stands in for. This one was calibrated
against the failure it prevents (resets) and never against the successes it
forbids, so there was no pressure to notice that it measures contiguity
while the guarded work needs volume. The general shape: when a proxy is
introduced because the real quantity is expensive to know, record what the
real quantity actually is, and measure it at least once — otherwise the
proxy's calibration point becomes the only evidence that will ever exist
about it.

**Related surfaces** — Studio's device card shows only "Memory free", which
is exactly the number that stays healthy while this failure happens; showing
`largest_free_block` (already on the heartbeat's `MemoryStats`) and the hole
count is follow-up 5 in the report.
