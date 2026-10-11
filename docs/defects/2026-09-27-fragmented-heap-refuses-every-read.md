---
status: open           # mitigated (the two-number gate); the fix is plan 2026-09-27-1218-fragmentation-tolerant-reads, PR B
found: 2026-09-27      # hardware walk of #854 on the PLAYFUL choker (Bluetooth on); prod refusals 2026-09-26
area: lpa-server ProjectRead gate (largest-block floor) × fw-esp32c6's two-region heap with Bluetooth on × edit-time residents in the heap's free tail
class: stand-in-divergence   # largest-free-block stands in for "can this read afford to run" — second time
related:
  - 2026-09-04-read-gate-refuses-on-largest-block-proxy.md
  - 2026-08-26-project-read-assembly-oom-resets-classic.md
  - ../adr/2026-08-28-project-reads-bounded-streamed-refusable.md
---
# A fragmented heap refuses every read while most of it is free

**Shape** — on the PLAYFUL choker (XIAO ESP32-C6, Bluetooth on), after a
few shader edits in Studio, every read the board is sent is refused:

```
read refused: heap headroom too low (largest free block 19478 B < 32768 B)
```

The editor stops updating and the device card goes stale. The board is
healthy: ~90 KB of its heap is free, it keeps rendering, and nothing
resets. Seen in prod on 2026-09-26/27 (19,478–19,480 B, every read), and in
the #854 hardware walk on the lab C6 with Bluetooth off, where the largest
block sat ~3 KB above the floor (35,095 B minimum over a 20-minute soak).

**Mechanism** — the 2026-09-04 entry's mechanism, on the C6, reaching a
user.

*The gate measures contiguity; the read needs volume.* The read gate
refused any read unless the heap had one 32 KiB free block. A read's whole
working set is 8.3–25.1 KB of small allocations it frees before the next
read (it keeps 0–176 B), and its largest single ask is 8 KB — a mapping
file's slot JSON on the first sync — and ≤ 2.5 KB for every editor or card
read (plan `lp2025/2026-09-27-1218-fragmentation-tolerant-reads`,
REPORT.md; `lp-emu:esp32c6:t1@4caa5b658` and `silicon:esp32c6
10:bd:a3:b0:8e:30` agree within 80 B).

*Edits fragment the tail; Bluetooth decides the floor.* Each shader
rewrite leaves a handful of 2–4 KB residents — the JIT module (2,744 B), a
`ShaderSlotDef` Vec clone (2,720 B), a 3,584 B Vec, the resolver's cache
(2,280 + 2,048 B), the source copy (1,971 B) — in the middle of region 0's
big free tail: 68.8 KB → 46 KB → 25–27 KB after one rewrite and restore.
Once that tail is cut, the whole heap's largest block is region 1's hole.
With Bluetooth off that is ~35.1 KB (just above the gate); with it on, the
controller's and host's allocations (~23 KB) cut it to ~19.5 KB — below
the gate, for good.

**Why it recurred** — the 2026-09-04 entry named this exact failure on the
classic and wrote the lesson down ("a one-scalar affordability gate is a
stand-in"). The fix direction was deferred as "its own session", the gate
stayed, and the C6 had enough contiguity to pass it — until Bluetooth went
on by default (2026-09-25) and took the margin.

**Mitigation (PR A of the plan)** — the gate asks two questions, per chip
(`lpa_server::ReadGate`, `LpServer::set_read_gate`): total free and a
small largest block. The C6 and S3 take 40 KiB free and a 16 KiB block
(the rule: total free ≥ the worst measured read working set + room for the
link and radio tasks; largest block ≥ 2 × the largest single read ask).
A slot value's sync JSON is now one allocation of its exact length, which
halves the 8 KB ask. The classic keeps its 32 KiB block (its heap has no
room for looser yet), so the 2026-09-04 entry stays open for it. The
refusal now says the board is busy and to retry.

On the emulated C6 with Bluetooth on and ten shader edits under the
editor's reads, the old gate refused 52 of 96 reads; the two-number gate
refuses none, with no resets. `lp-cli/tests/emu_frag_reads.rs` holds that
in CI ("Emulator C6 (x64)", `just test-emu-c6-cli`).

⚠️ **Residual risk until the fix.** Reads still allocate infallibly, so a
board whose largest block is 16–32 KiB can now *reset* on a read with a
single ask over ~15 KB — a slot value over ~15 KB of JSON (catalog max
8.9 KB), a display layout over ~800 lamps, a render probe over ~2,048 px —
where it used to refuse.

**Fix (PR B of the plan, open)** — reads write each event from engine
state straight into the board's static frame buffer, nothing data-sized is
built on the heap, what is left is fallible and answers a structured
"busy" that Studio treats as transient, and a heap-ratchet ceiling pins
the read's largest allocation. Per-chip gates are re-measured on all three
emulators. This entry closes `fixed` with it.

**Lesson** — a recorded lesson is not a fix. The 2026-09-04 entry was
right about the mechanism and its fix direction, and the failure still
reached a user three weeks later on a different chip, because nothing
tied the deferral to the condition that would make it bite (a board with
less contiguous room). When a defect's fix is deferred, say what change
would make it user-visible — here, anything that takes contiguous heap on
the C6, like turning the radio on — so that change's review can find it.
