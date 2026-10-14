---
status: fixed
found: 2026-10-09        # e2e: `just walk-wifi-emu relay`, while adding relay protocol 2's steps
fixed: 308cdea34
area: lp-cli tests/emu_relay_link.rs × scripts/emu/walk-wifi-emu-relay.mjs
class: unenforced-test-precondition
related:
  - docs/reports/2026-10-09-relay-pictures-emulator-walk.md
  - docs/reports/2026-10-07-wifi-relay-emulator-walk.md
  - lp2025/2026-10-08-2050-pictures-through-the-cloud (P6)
---
# The relay walk's takeover check read a line the board had already dropped

**Symptom** — `just walk-wifi-emu relay` failed on its R4 check with the
cell itself green:

```text
✗ R4 the same key takes the session on the LAN — missing: /closed \(taken over by the same key\)/
```

The unmodified cell failed the same way on this tree, twice
(`[serial] [LINK] 68 log records dropped`, then `72 log records dropped`,
and no `taken over` line in either log). The lane passed when it was
written (2026-10-07).

**Root cause** — the board's console reaches a host only over its USB
link. With no link open, its log records wait in a 4 KB ring
(`fw_esp32_common::log_ring_logger::LOG_RING_BYTES`) that keeps the newest
and drops the oldest. Step 4 of the cell opens no USB link: the takeover
line is said, then Bob's eight busy routes (three lines each) and the
heartbeats while the heap is read, then a deploy, before step 6 opens the
next link. By then the ring had overwritten the takeover line. Whether it
survived depended on how much the board said in between, which the cell
never bounded; more logging since 2026-10-07 tipped it over.

**Fix** — step 4 holds a USB link while the takeover happens (the cell's
`UsbConsole`, which hears the board's console through the device session's
events), waits for the board to say `closed (taken over by the same key)`,
and closes the link before the heap is read over the LAN session, so that
row still has no USB link open. The relay protocol 2 steps that wait for
the board's words hold a link the same way.

**Regression coverage** — the cell now fails, with the board's console in
the message, if the board never says the takeover line; the lane's R4 reads
it.

**Lesson** — a walk that reads a board's console must hold the link that
carries it while the words are said. A line spoken with no host attached
is buffered on a best-effort basis, and "it was there last week" only
means the buffer did not overflow that time.
