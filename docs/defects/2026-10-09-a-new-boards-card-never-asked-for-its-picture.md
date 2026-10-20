---
status: fixed
found: 2026-10-09      # how: e2e (walk-ble-emu's card step, emulated, lp-emu:esp32c6:t1)
fixed: this change
area: lpa-studio-web app/board_card/board_card.rs (the picture lease)
class: assumed-context
related:
  - lp2025/2026-10-08-2050-the-board-card (P10, found merging #1062)
  - docs/defects/2026-10-09-a-phones-bluetooth-link-leaves-the-choker-under-the-read-gate.md
---
# A new board's card never asked for its picture

**Symptom** — `just walk-ble-emu --serve-release` on the board card branch:
a board added over Bluetooth identified, took a project and said `Project
loaded`, but its card's picture stayed dark for three minutes and the status
corner's details said "No picture yet — the live feed is coming." The card
step (#1062's, waiting for "live · … · shown 1–2/s") timed out.

**Root cause** — the board card leases its board's picture while it is on
screen (`DeviceFeedOp`, the feed pulls only for wanted cards), and it decided
whether to lease once, at mount: a new board's card (presence New) asks for
nothing, so its lease was skipped. A board that says who it is keeps its
handle (adoption keeps the id), so the home page keeps the SAME card mounted
as it turns from New to Online — and the card never asked again. Today's
card hid this because a new board was a different component
(`PendingLinkCard`), so the roster card mounted fresh, lease and all. Every
board that arrives as a new board (a fresh USB or Bluetooth connect) had no
picture until something remounted its card.

**Fix** — the lease follows the card (`lease_change` in `board_card.rs`): it
is re-read whenever the card's presence or board changes, starts when a new
board's card turns Online, moves when the card is handed another board, and
is released on unmount.

**Regression coverage** — `the_picture_lease_follows_the_cards_presence`
(the rule), and `walk-ble-emu`'s `card` step (the picture live at the
Bluetooth pace on a board added over Bluetooth).

**Lesson** — a component kept mounted across a state change keeps its
mount-time decisions. When one card stands for an object through its whole
life, anything it asks of the world at mount has to be asked again when the
object changes what it is.
