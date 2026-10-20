---
status: open
found: 2026-10-09      # hardware-walk: Yona's iPhone (Bluefy) on loose-c6, the BLE card picture's phone check (PR #1062)
area: lpa-server ProjectRead gate (total-free floor) × fw-esp32c6 heap with a Bluetooth central connected × lpa-studio-core card feed (refusal reason dropped)
class: partial-knowledge-loss   # the board says why; Studio drops it and the card waits
related:
  - 2026-09-27-fragmented-heap-refuses-every-read.md
  - 2026-09-04-read-gate-refuses-on-largest-block-proxy.md
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
  - ../adr/2026-09-24-ble-transport.md   # 2026-10-08 amendment: the card's picture over Bluetooth
  - lp2025/2026-09-27-1218-fragmentation-tolerant-reads
---
# A phone's Bluetooth link leaves the choker under the read gate, and the card only says "Waiting"

**Symptom** — Yona's iPhone (Bluefy) connected to `loose-c6` (a XIAO
ESP32-C6, release 2026.10.08-23) running the PLAYFUL Choker (lab rehearsal).
The device card said "Waiting for the first frame…" and never drew a
picture. The board's console refused every read, 104 of them in a few
minutes:

```
read refused: board memory busy (free 40636 B, largest block 23324 B; needs 40960 B free and a 8192 B block); retry shortly
```

The board rendered on at 33 fps, and the link stayed up. Nothing in Studio
said why the picture never came.

**Root cause** — two halves.

*The board's floor.* `READ_GATE` on the C6 asks 40 KiB of total free heap
before any ProjectRead (`lp-fw/fw-esp32c6/src/main.rs`), sized for the worst
measured read's working set (9–25 KB) plus room for the link and radio
tasks. A connected Bluetooth central costs ~17.5 KB of heap (the link's
7,112 B plus the controller and host). The choker leaves ~61.7 KB free on
a fresh boot. That is ~43.8 KB with a phone connected, 2.8 KB over the line.
On this walk the board had switched projects at runtime the night before
(Logo Sign, then the choker), and ~3 KB of total free had not come back:
58.8 KB without the phone, 41.2 KB with it, 40.6 KB as a read arrived.
So it was under. Fragmentation was not what refused: the largest block
(23 KB) cleared the 8 KiB floor easily. A reboot brought it back
(43.8 KB free with the phone, every read answered).

The one floor is the worst read's, applied to every read. The card's read
is one output-frame probe whose reply is under 1 KB for the choker, and it
is refused for the editor's sake.

*Studio's half.* The refusal reaches Studio as a structured terminal error
with the numbers in it, and the heartbeat carries `freeBytes` and
`largestFreeBlock` every 5 s. The card feed counts the refusal as a failed
pull (`DeviceFrameFeed::note_failure`, a `log::debug!`), and after three it
parks. The card says "Waiting for the first frame…" until then, and "No
picture yet — the live feed is coming." after. Neither is true, and neither
says memory.

**Fix** — none yet. Candidates, in the order proposed to Yona (2026-10-09):

1. Say it: keep the refusal's reason on the feed (and the lens), and have
   the card say the board is low on memory, with the numbers, beside a
   **Restart board** offer the person presses (an offer built in core). A
   parked feed must not promise "the live feed is coming".
2. Size the floor to the read: the fragmentation-tolerant reads plan's PR B
   (reads cap their own allocations) is where a small read stops being held
   to the worst read's floor.
3. Find the ~3 KB a runtime project switch leaves behind.

An automatic reboot was considered and not proposed: here it bought ~3 KB
at the cost of the show blinking and the phone's link dropping, and left
the board 2.8 KB over the line.

**Regression coverage** — none: no test puts a central's heap cost beside
the read gate. `lp-cli/tests/emu_frag_reads.rs` holds the fragmented case
in CI, but the emulated board's Bluetooth never connects (no central), so
its free heap never takes the ~17.5 KB a phone costs.

**Lesson** — a fixed total-free floor plus a variable radio cost can leave
almost nothing between them. With the choker it was 2.8 KB on a fresh boot,
which is no margin at all for a board people edit live. Every radio link a
board holds belongs in the floor's arithmetic. When the board refuses for a
reason it can name, the person needs to see that reason, not a wait.
