---
status: carried
since: 2026-08-01      # #243, the first optimisation to be caught by it
logged: 2026-08-02
area: lpc-engine (dataflow, node runtimes) + the classic ESP32 image
related:
  - ../defects/2026-08-01-classic-heap-regression-after-f32-merge.md
  - ../defects/2026-08-01-classic-rmt-open-fault.md
  - ../adr/2026-08-01-esp32v3-flash-budget.md
  - s3-frame-cost-scales-per-fixture.md
---
# Per-frame optimisations land with a cycle number and no byte number

**Shape** — the engine's frame cost is under active, measured attack, and
the standard shape of a win is "compute it once, keep it": persistent
resolution (#243), the resolver route table, interned query ids, per-frame
value caches, the fixture LUTs. Every one of those is a *trade* of resident
memory for cycles, and the review record for them contains a cycle figure
and no memory figure. That is not an oversight by any individual change: the
host has gigabytes, the S3 has 16 MB, the C6 has room, and CI measures
neither. The one part where the trade can be wrong — the classic ESP32, with
a 112,640 B arena — is also the part nobody profiles on.

This is structural, not a bug. Nothing in the workflow makes the byte cost
of a per-frame cache visible at the moment the trade is made.

**Carrying cost** — #243 cost the classic ~8.3 KB of per-project heap, about
90 LEDs of a ~240-LED ceiling, and it took a full day of hardware bisecting
(fourteen build/flash/measure cycles on the one desk board) to attribute,
because the defect record's candidate list was assembled from plausibility
rather than measurement and named the wrong four PRs. The same shape cost
the classic once before, in the per-channel white-point LUT. Each incident
burns a hardware session and blocks whatever gate the board was needed for.

**Workarounds**

- `fw-esp32v3` prints `[MEM] free=… used=… largest_free=…` once per
  heartbeat, and `log_memory` brackets project load stage by stage. Diffing
  the `used` figure at "project new after core project" across two firmware
  builds brackets a per-project cost without needing a crash.
- Bisect by merging candidate `main` commits into a fixed classic-firmware
  branch point rather than checking out `main` — `fw-esp32v3` does not exist
  on `main` before #239, so a naive `git bisect` walks straight off the
  crate.
- `espflash reset` is a power-on-class ledger wipe: the clean way to void a
  path quarantine between measurements.
- A cache whose cost is unacceptable on one part can become a removal-only
  Cargo gate (`resolver-payload-cache` is the worked example) rather than a
  revert. Split the cache along the line between *decisions* and *payloads*
  first: on the resolver, the decisions were 11 ms of the 24 ms for zero
  bytes, and only the payloads cost RAM.

**Incident log**

- **2026-08-01** — per-channel white-point LUT: classic-only per-project
  growth, found on the bench.
- **2026-08-01** — #243 persistent resolution: −8,136 B on the classic,
  +54 % fps. Filed as `classic-heap-regression-after-f32-merge`, initially
  misattributed to the three f32 PRs.
- **2026-08-02** — bisected and gated. The classic now runs decisions-only
  at 18,144 B / 16 fps, ahead of its pre-#243 18,128 B / 13 fps; flipping the
  gate back on costs 8,368 B for 5 fps.
- **2026-09-02** — the emulator heap-budget gate (`docs/heap-budget-gate.md`)
  now prices a per-frame cache in bytes without a serial cable: `fw-emu`
  turned `resolver-payload-cache` on to match the C6, and the record diff
  showed `examples/basic` `frame.retained` +15,361 B (30,007 → 45,368 B).
  Same gate gained `alloc_count` / `alloc_bytes` per frame (a steady-render
  pass), so the *churn* side of a per-frame optimisation is priced too. Partial
  exit: the number is produced by CI and moves when a cache is added; it is
  the C6 configuration, not the classic's arena.

**Exit criteria** — a per-project heap number for a known project on the
classic (or a faithful emulation of its arena) is produced by something
other than a person with a serial cable, and moves when a cache is added.
Concretely: either `fw-checks` grows a heap-budget check that a CI hardware
run can assert, or the host test suite gains a tracking-allocator harness
that reports resident bytes after loading `projects/test/quad-strips-v3`,
with a recorded baseline that a PR has to update deliberately. Until one of
those exists, every per-frame optimisation is priced in one currency.
