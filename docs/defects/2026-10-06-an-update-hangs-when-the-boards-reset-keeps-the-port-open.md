---
status: fixed
found: 2026-10-06      # how: e2e (`just walk-ota-emu`, the emulated C6 over `?emu=`)
fixed: 639edfcde
area: lpa-studio-core app/devices/update_host.rs, device_effects.rs (the link pump)
class: assumed-context
related:
  - docs/adr/2026-10-06-studio-updates-over-the-update-channel.md (decision 3)
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (P9)
---
# An update hangs when the board's reset keeps the port open

**Symptom** — The first `just walk-ota-emu` run: X → Y, one press. The card
said "Backing up current firmware…" and reached 100 %. The board said
`[OTA] offer … → core`, committed the core and restarted. Then the card
stayed on its last line for the rest of the run. The board was back on a
new lp-link session, waiting for a host that never spoke to it again.

**Root cause** — The update host ended a leg only when the model saw the
link *close*, and it brought the driver up only on a new leg. Over the
emulator's door, a board's restart is an lp-link **session reset on a port
that stays open**. The bench C6 on a Mac later showed the same thing on
silicon: its port stayed open through all three of an update's resets. So
there was no close and no new leg. The driver kept its old session's state
and waited for words that belonged to that session. The board's `M` on the
new session went to a driver that did not expect one.

**Fix** — The pump now reports a link-reset note to the update host
(`UpdateHost::on_link_reset`), and the driver goes down there. The board's
first word on the new session brings the driver back up (`session_back`),
and the leg carries on. That first word is either its `M` (a core-only
board sends one unasked) or a hello that announces channel 3 (`on_hello`;
a running engine sends no `M` unasked).

**Regression coverage** — `studio_update_e2e_tests::an_update_whose_resets_keep_the_port_open_ends_on_y`
(it uses the board double's `resets_keep_port`), and the walk's `update`,
`cut-core` and `cut-engine` steps. On silicon (2026-10-06): three Studio
updates of the bench C6 over a door bridge. Each run reached
`[OTA] core confirmed` and the card's "… same as this Studio".

**Lesson** — On lp-link, "the board restarted" and "the port closed" are
different facts, and the transport decides which one a reset produces. Any
flow that spans a board restart must key on the link's session, not on the
port's life.
