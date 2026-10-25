---
status: fixed
found: 2026-10-24      # how: user report (Yona updating FS3 over Bluetooth from Bluefy on his iPhone)
area: lp-app/lpa-update update driver (drive/update_driver.rs, the Progress effect) × Studio's board card
class: assumed-context   # the driver presumed a piece's requests only move forward through it
related:
  - lp-core/lpc-update/src/board/piece_stage.rs (the engine header held, written last)
  - docs/adr/2026-10-06-ota-update-protocol.md
---
# An update's progress went back at the end

**Symptom** — the update finished, but right at the end the card's number
went 100 % → 50 % → done.

**Root cause** — the board writes a piece's chunks in order from its first
unwritten one, except the engine's header chunk (offset 0): it holds that
one and asks for it last, so nothing looks like a valid engine while the
engine's bytes change. The driver reported each request's offset as how far
the piece had come (`done: r.off`), so that last request read as the engine
at 0 %, which the card's one number (the engine is 50–100 %) said as 50 %.
`lpa-update`'s sim showed it on a clean run: `Finishing 36864 / 37864`, then
`Finishing 0 / 37864`.

**Fix** — the driver reports the furthest offset asked for in each stage.
The sim asserts that every clean case's progress only moves forward; a core
end-to-end test (`an_updates_one_number_only_moves_forward`) walks an update
through Studio's controller and checks the card's number, start to end.
Both fail without the fix.
