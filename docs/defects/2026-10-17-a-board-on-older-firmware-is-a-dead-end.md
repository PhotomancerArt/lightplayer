---
status: fixed
found: 2026-10-17      # how: Yona, on lightplayer.app, with the desk's fixture C6 (FC6, A0:F2:62:87:B4:8C) on USB
fixed: 2026-10-17      # PR #1176, the board card's top says what matters (round 3)
area: lpa-studio-core `board_card/primary_action.rs`, `board_surface.rs`, `project_bar.rs`, `device_flash_offer.rs`; lpa-studio-web `board_card/`
class: state-conflation
related:
  - docs/ux/anatomy/board-card.md ("Surface")
  - docs/defects/2026-10-16-a-board-with-no-firmware-says-project-and-access-not-known.md
---
# A board on older firmware is a dead end: a black picture, no next step, and a false "Project none"

**Symptom** — the desk's fixture C6 runs a 2026-10-03 build (wire 33) and
Studio speaks wire 43. The board was otherwise fine: it played `studio` at
32 fps. Its card on lightplayer.app showed a black picture, no primary
button, "Project none" (false: it was playing a project), and the fix as a
quiet "older LightPlayer" aside on the Firmware row, with a red, two-click
Update. Yona: "this board is connected by usb, but I can't connect/edit
it? feels like a dead end… I'm guessing maybe I need to update the
firmware, but I don't want to guess, I want to be told."

**Root cause** — three conflations on one card.

- The primary only knew `flash` as the way forward for a board that wants
  a flash. An older LightPlayer whose board resolves is offered
  `update-firmware` instead (`firmware_verb`), so the primary fell through
  to "ready board, Connect", which an older LightPlayer never is: no
  button at all.
- The Project row read "none" for every face that wants a flash, the
  LightPlayers Studio can't read among them: "can't read what it plays"
  and "plays nothing" were one state.
- The update was priced as the Flash op's Lasting ("Replace what this
  board runs?"), though for a LightPlayer it keeps the project and files:
  the red read as "broken", the two clicks as danger.

And nothing on the card's top said any of it: the picture was its only
content, and a board that shows none is black.

**Fix** — the card's top is its main surface (`UiBoardSurface`): while the
board is well it is the picture, otherwise the state's icon and plain
sentences ("This board is running old firmware. Update the firmware to
connect and edit."), the picture dimmed beneath. The primary reads the same
state, and an older LightPlayer's is **Update firmware**. The Project row
says "not known". The update is Routine (one click, no red; the agent may
press it): it keeps the project and the files, as the over-the-air update
does. Core tests press the primary by path, on a fake board speaking a
real wire-32 hello (`an_older_lightplayers_card_updates_its_firmware_by_path`).
