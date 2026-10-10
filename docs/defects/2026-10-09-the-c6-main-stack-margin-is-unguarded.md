---
status: open
found: 2026-10-09      # report: the whole-RAM ledger (scripts/ram-ledger.py, RAM research E1) read the C6's stack off CI's image at 3c7e838c2
area: fw-esp32c6 main stack (esp-hal stack.x residual) × scripts/heap-budget-check.sh (the chip gate's stack arm)
class: budget-exhaustion   # a hard budget (the stack) is gated against the wrong quantity, so growth of the statics crosses the real limit silently
related:
  - ../adr/2026-09-02-esp32c6-ram-split.md   # the stack is a measured number; its 2026-09-24 amendment (BLE ate 24 KB of heap to keep it)
  - ../adr/2026-09-23-heap-budget-record-split-and-derived-stack.md
  - ../heap-budget-gate.md   # "The stack's size is derived, not recorded"
  - lp2025/2026-10-09-1203-ram-research   # E1 report; E4 (stack into the reclaimed segment), E7 (gates against real peaks)
---
# The C6's main stack has no gate that knows what the flagship project needs from it

**Symptom** — nothing has failed. The numbers say a failure is closer than any
gate does. The C6's main stack is the residual of RAM after the statics, and
it has shrunk with every feature that added a static:

| date | main stack | source |
|---|---:|---|
| 2026-09-02 | 72,768 B | `docs/adr/2026-09-02-esp32c6-ram-split.md` |
| 2026-09-07 (`735af98ae`) | 71,512 B | silicon capture's `stackTotal`; also read off the pinned CI image by `scripts/ram-ledger.py` |
| 2026-09-24 | 62,664 B | the ADR's BLE amendment |
| 2026-10-05 | 56,184 B | the same ADR's placement amendment |
| 2026-10-09 (`3c7e838c2`) | **49,008 B** | `just ram-ledger` on `ESP32C6_SERVER_RADIO_SPLIT/p2.elf`; `scripts/heap-budget-stack-layout.py` agrees |

Against that, the last measured **meteor** (the flagship, a compute-shader
project) steady-state high-water is **36,936 B** (silicon, XIAO C6, 2026-09-02,
firmware `4e463d805743`) and **33,896–35,496 B** (`lp-emu:esp32c6:t1`,
2026-09-24). On those numbers the margin at `3c7e838c2` is about 12 KB of 49 KB,
and it fell 13.7 KB between 09-24 and 10-09. **Meteor's high-water has not been
re-measured on the current image**, and no other project's has been measured
at all (the PLAYFUL Choker, Zook dome, a project with a Bluetooth central
connected).

**Root cause** — the gate checks the wrong quantity. `heap-budget-check.sh`'s
stack arm (docs/heap-budget-gate.md, "The stack's size is derived, not
recorded") requires only that the derived stack total still exceed the top of
the **recorded high-water band**, and that band is the first heartbeat's idle
figure (12,168 B, band 11,600–12,700 B in
`scripts/heap-budget-record/chips/esp32c6.json`). On the 49,008 B stack that is
"36,308 B above the high-water band" (49,008 − 12,700). The stack could lose another 36 KB — to a point
where meteor has been overflowing for 24 KB — before that arm fails. Statics
growth is "visible as the stack shrinking, which the run prints", and nothing
turns a printed number into a failure. The 22.5 KB that went from 09-07 to
10-09 was spent in ordinary, individually reasonable PRs (the ledger's
09-07 → 10-09 diff: BLE controller code in RAM +20,708 B, `.bss` +16,755 B
led by `engine_task::POOL` +6,088 B, `LOG_RING` +4,116 B, the BLE host's
`DEFAULT_POOL` +4,036 B; the heap gave back 24,000 B in the BLE cut, and the
stack took the other 22,504 B) and none of them crossed a line.

A stack overflow on this layout writes into the statics below the stack (the
2026-09-02 ADR's account: "silent corruption until the scheduler happens to
sample the pointer"), which is the failure the stack probe was added to keep
from being a surprise.

**Fix** — open. Candidates, none chosen:

1. Gate the chip check on `stackTotal ≥ (a recorded steady-state high-water
   of the flagship) + a stated margin`, measured with a project loaded on the
   emulator, not at the first heartbeat. RAM research E7 measures the real
   peaks this would be graded against.
2. Put the stack where it does not compete with the statics (RAM research E4:
   the reclaimed bootloader segment), which removes the quantity rather than
   guarding it.
3. A ratchet on `stackTotal` itself (the stack may shrink only by a stated
   amount per PR), the way `lint-web-actions` ratchets web-built actions.

**Regression coverage** — none: nothing is broken yet.

**Lesson** — a residual is a budget nobody owns. The stack was made "derived,
not recorded" so that adding a static would not force a re-baseline; the cost
was that nothing records how close the derived figure is to what the workload
needs. Gate a derived quantity against the *demand* on it, not against the
smallest value that still boots. The same shape applies to the classic
(37,072 B stack) and the S3 (32,432 B): both are gated against a boot-time
high-water (12,860 B and 13,088 B in their chip records), and what a project
running on them needs from the stack is not recorded anywhere this entry
could find.
