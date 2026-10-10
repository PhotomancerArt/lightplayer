---
status: fixed
found: 2026-10-09      # how: report (the RAM research program's choker heap record)
fixed: this change
area: lpc-engine (PowerButtonNode::ensure_input), lpc-hardware (VirtualButtonDriver::endpoints)
class: unbounded-restatement
related: [2026-07-28-tick-error-restated-every-frame.md]
---
# A power button that could not open was reopened, and reported, every frame

**Symptom** — The PLAYFUL choker's engine heap record
(`scripts/heap-budget-record/engine/catalog/projects/playful-choker-tryout.json`)
measured a steady frame of **41,887 B transient, a 20,480 B largest
allocation, 1,844 allocations and 76,543 B requested** on `fw-emu`
(`lp-cli profile --collect alloc`, engine emulator, recorded 2026-10-07 at
`7fa64fd84`). A steady render frame of a 73-LED project should not allocate a
20 KB block. The RAM research program (E8) named it a "missing-D0 retry
artifact" and asked what it was.

**Root cause** — `fw-emu`'s manifest has no `button:local:D0`, and the choker's
`power.json` names it. `PowerButtonNode::ensure_input` tried to open the
button on **every frame** it was not open, and an open that failed left no
memory of failing. Each attempt went through
`HardwareSystem::open_button_by_spec` → `find_endpoint`, which asks the virtual
button driver for `endpoints()`: a `Vec<HwEndpoint>` of every GpioInput
resource on the board (20 KB on `fw-emu`'s 256-resource manifest), searched for
a match that is not there. Then the failure was formatted into a `String` and
returned as a node error, so the server's tick-error ledger (the 2026-07-28
fix) quietened the *log* while the *work* ran sixty times a second. The heap
record ratcheted that artifact instead of the project. On silicon, a project
naming a button the board lacks churns the same way, smaller (the C6's
manifest is smaller).

**Fix** — The node remembers the open it lost (`OpenFailure`: what it asked
for, the error text, when to ask again). A failed open is retried after
500 ms, then 1 s, 2 s, 4 s, and every 4 s after that, or at once when the
node's endpoint, mode or `stable_ms` changes. Time comes from the tick
context (`now_ms`), as everywhere in the node; nothing reads a clock. The
failure is returned as a node error once per distinct message; a retry that
fails the same way is silent, and a changed config or a changed message
reports again. The 500 ms / 4 s pair is a judgement: the board's endpoint set
is fixed at boot, so a missing button is almost always missing for good and the
cap keeps the cost at one failed open every 4 s (about 0.25 per second instead
of 60), while the short first wait still finds a driver that binds a moment
late. The happy path also stopped cloning the endpoint every frame to compare
it (`OpenedPowerButton::matches`).

**Not done** — The brief also asked to retry at once when "the board's
hardware/resource set changes". `ButtonService` has no change signal and the
manifest does not change after boot, so there is nothing to subscribe to; a
config edit is the only trigger.

**Regression coverage** — `a_failing_open_is_retried_on_a_backoff_and_at_once_on_a_config_change`
and `the_backoff_stops_doubling_at_its_cap` in
`lp-core/lpc-engine/src/nodes/power_button/power_button_node.rs`, which count
opens through a wrapping `ButtonService`.

**What the heap record shows** — Per-frame allocations in the choker's
steady-render capture (`lp-cli profile --collect alloc --mode steady-render`,
`playful-choker-tryout`, engine emulator, `A` events per frame): before, 1,513
in each of the four captured frames; after, 226 in three of them and 1,512 in
the fourth, which is the one frame that falls on a retry. The record keeps the
*worst* frame of the window, so it still reads a retry frame: transient
41,887 → 41,872 B, allocations 1,844 → 1,843, largest allocation unchanged at
20,480 B. The record did not collapse because the fix leaves one attempt per
backoff, and four 40 ms frames are enough to contain one. A retry is still a
20 KB transient; making it cheap needs `find_endpoint` to stop building the
whole endpoint list to find one match (out of scope here).

**Lesson** — The 2026-07-28 entry fixed the *restatement* and left the
*retry*: a tick error that is the same every frame is a condition, and a
condition that is re-attempted every frame is a workload. When an error is
rate-limited at the edge, check what the code behind it still does per frame.
And a heap record taken on an emulator whose manifest lacks the project's pin
measures the failure path; a number that looks like a project's cost can be
the cost of an error.
