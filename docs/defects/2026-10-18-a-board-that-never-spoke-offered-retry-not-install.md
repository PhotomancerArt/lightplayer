---
status: fixed
found: 2026-10-18      # how: a first-time user on lightplayer.app, flashing a new board
area: lpa-devices `Device::status`; lpa-studio-core board card (`board_surface.rs`, `connection_bar.rs`)
class: state-conflation
related:
  - docs/ux/anatomy/board-card.md
  - docs/ux/README.md   # P19's bad → do pair
---
# A board that never spoke LightPlayer offered Retry, not Install

**Symptom** — a first-time user on lightplayer.app plugged in a new board
whose factory firmware prints nothing on USB, and set it up from its
new-board card. The card said "This board stopped responding over USB." /
"Retry to reconnect.", with Retry as its primary and "USB · not responding"
in orange on its Link row. Retry asked again and got the same silence: a
dead end. In their words: "'board not responding' is sort of a 'duh, I'm
trying to program it, yes I want to flash it'".

**Root cause** — `Device::status()` made every silent open port
`DeviceStatus::NotResponding`, whether or not the board had ever spoken
LightPlayer: one status for two facts, a LightPlayer that stopped answering
and a chip whose firmware never says anything. The card's surface checks
`NotResponding` before the firmware face, so the state the surface table
already had for this board — "Says nothing": "This board doesn't respond.
Install the firmware to use it.", Install firmware, from
`FirmwareFace::Silent` — was unreachable on a board's card. "Stopped
responding" was also untrue of a board that never spoke. (Its new-board card,
left alone, read the face and offered Install; pressing "Set up this device"
while it identified is what made it a board card.)

**Fix** — `Device::status()` keeps `NotResponding` for a board that has said
hello (`Device::spoke_light_player`: a hello in the current window, or the
record's firmware label, which only a hello writes — on this link earlier,
or remembered by MAC from another session). A board that never said hello
and settles silent is `NeedsAttention`, as a blank chip is: the card reaches
"Says nothing" with Install firmware (`flash`) as its primary, and its Link
row reads "USB · connected", quietly. A MAC is not the evidence: the flash
preflight learns a blank chip's MAC from its ROM.

**Regression tests** — `lpa-devices` `device.rs`:
`a_board_that_never_said_hello_and_says_nothing_needs_firmware`,
`a_board_that_said_hello_and_went_quiet_is_not_responding`. `lpa-studio-core`
`studio_device_e2e_tests/board_card_tests.rs`:
`a_silent_board_that_never_said_hello_offers_install_not_retry` (presses
`flash` by path), `a_silent_new_boards_card_offers_install`, and the
existing `a_not_responding_boards_retry_presses_retry_by_path`. Story:
`board_card_says_nothing`.
