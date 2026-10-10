---
status: open
found: 2026-10-10      # how: hardware-walk (RAM research V1, CX1 joined to a home Wi-Fi network)
area: lpa-update `backup` (4 KiB read-back chunks) × lpc-update's board session × the C6 heap after a busy LAN session
class: budget-exhaustion
related:
  - docs/adr/2026-10-06-ota-update-protocol.md
  - docs/defects/2026-10-08-a-recompile-on-a-fragmented-wi-fi-heap-resets-the-board.md
  - lp2025/2026-10-09-1203-ram-research (experiments/v1-1092-joined-wifi/report.md, "Found that the brief did not predict")
---
# A LAN update that starts after a busy session resets the board as the backup begins

**Symptom** — on the shipped release image (2026.10.10-7, `a52b5e0b485e`)
on silicon CX1 (ESP32-C6, 14:C1:9F:E6:54:90) joined to a home Wi-Fi
network, an over-the-air update over the LAN (`lp-cli link capture lan:…`
with the update offer, no USB host) that starts after a busy session (800
back-to-back project reads) resets the board 3.8–4.3 s into the backup.
The board's recovery record, read after the reset, says:

    lastCrash: cause "oom", path "<no frame>",
    message "alloc 4102 bytes failed (align 1) in <unset>"

Just before it the heartbeats show ~27 KB free with an ~8 KB largest block
(`freeBytes` 27,312, `largestFreeBlock` 8,044). The host reconnects and the
update completes (UpToDate in 96–124 s). It reproduced on the release
image: the control run (a freshly flashed release, the same 800-read
session, release to X) reset 4.3 s into the backup, and the PR image under
test (#1092) reset 3.8 s in. The PR does not cause it. A fresh boot with no
preceding session (the PR image's Y to X) did not reset.

**Root cause** — not diagnosed. What the evidence fixes: the backup reads
the running engine back in 4 KiB chunks (`lpa-update`'s backup session,
`off: 4096, len: 4096`), the failed ask is 4,102 B, and after the busy
session the board has ~27 KB free in total but only an ~8 KB largest block.
The suspect is the read-back's 4 KiB reply (payload plus a few bytes of
framing) being built in one block alongside the link's other buffers; that
is a reading of the sizes, not a traced path. The allocation is infallible,
so it aborts; the context is `<unset>` because the update session is not a
node. The update ADR's Wi-Fi backup timings were taken on a fresh heap.

**Fix** — none yet. Options:

- Let the board answer a read-back with less than the host asked when the
  largest block is short (the link already asks for smaller chunks over
  Bluetooth), and let the host take the smaller size.
- Make the read-back's buffer fallible, so a short heap is a refusal
  the host retries smaller, not an abort.
- Return the session's kept memory to the big free tail before an update
  begins.

**Regression coverage** — none yet. The sequence is 800 back-to-back reads,
then the update; it needs a Wi-Fi heap, so the emulated LAN (`net=lan`) is
the place to make it a test.

Evidence (RAM research V1,
`experiments/v1-1092-joined-wifi/evidence/ota/` in
`lp2025/2026-10-09-1203-ram-research`): `A-pre-reads.txt` (the 800 reads),
`a-rel-to-x.txt` and `a-rel-to-x-lpcli.log` (release control), `x-to-y.txt`
and `x-to-y-lpcli.log` (PR image); the `lastCrash` record is in the
heartbeats of each run.

**Lesson** — the update protocol was sized and timed on a fresh board. An
update is what a user runs after a long editing session, so its first ask
has to be sized to the heap the board has then, not the one it boots with.
