# Board card

One board: what it plays, how it's reached, what you can do, what runs it.
Shown on the home page and the Boards tab. Built in core
(`lpa-studio-core/…/board_card/`), drawn by the web
(`lpa-studio-web/…/board_card/`).

Status: **shipped** (round 2 of the card's words, 2026-10-16, PR #1159;
plan `lp2025/2026-10-16-1844-card-words-round-2`). Where a line changed
what shipped before, its comment says `was:`. ◆ is the layers icon (O2).

## Whole

```text
┌──────────────────────────────────────────┐
│                               ╭──────────┤
│            picture            │  corner  │   1
│                               ╰──────────┤
├────────────────────────────┬─────────────┤
│ Porch lamp                 │ ↔ Connect   │   2 name · 3 primary
├────────────────────────────┴─────────────┤
│ ◆ Project   studio                     › │   4 rows
│ ↔ USB · connected                      › │
│ ○ Access    edit · USB                 › │
│ ▲ Firmware  v2026.10.16-1              › │
│ ■ Hardware  XIAO ESP32-C6              › │
└──────────────────────────────────────────┘
```

```text
1 corner    the picture's state. empty while live.                 P4
2 name      the board's own name. rename lives in Hardware's details.
3 primary   the one next step, or nothing at all.                  P7
4 rows      one fact each, always in this order.                   P8
```

Connected, the rows become the board's panel and the primary becomes Done:

```text
├────────────────────────────┬─────────────┤
│ Porch lamp                 │ ✓ Done      │
├────────────────────────────┴─────────────┤
│ Brightness  ━━━━━━━━━━━━━━━━━──────  72% │   the master fader
│ ◎ Speed    ◎ Hue    ◎ Width   ▣ Strobe   │   one row of controls
│ All controls · 3 more            │ Edit  │
└──────────────────────────────────────────┘
```

## The row

```text
│ ◆ Project   studio        out of date │ Send latest │
  │ │         │             │             │
  │ │         │             │             action   verb (+ object). fixes the state. at most one.
  │ │         │             state         only when not normal. its tone says how bad.
  │ │         name          the thing itself. here in every state.
  │ kind                    muted. teaches what the row is about.
  icon                      the kind's one icon, the same everywhere in the app.
```

```text
›          no action: the row opens its details (everything the row leaves out)
tone       none · orange: needs you · red (striped): broken · blue: live, the lit ☁ only
           (ui.md's status families; the card adds none, P16)
working    ◌ takes the icon's place; progress runs along the row's foot ━━━━────
stale      the row dims; its words don't change
```

The Link row is the one exception: the link's own icon and name are its
kind (`USB`, `Wi‑Fi`, `Bluetooth`, `lightplayer.app`), as a version
number is.

## Each row

### Project ◆: what the board plays

```text
◆ Project   studio                                   # was: studio, with no kind word
◆ Project   studio         2 boards
◆ Project   none                                     # primary: + Add a project
◌ Project   studio         sending · 40%             # bar along the foot
◆ Project   studio         out of date │ Send latest │      # no tone (P16). was: Out of date · studio, orange
◆ Project   studio         error                     ← red   # details: the error
◆ Project   none           data lost  │ Restore │ Reset │   ← orange   # O3. was: on the Firmware row, "Its files are gone"
◆ Project   hidden                                   # locked. was: Project not known
◆ Project   studio                         (dim)     # offline. was: Project not known
◌ Project                                            # the board hasn't said yet
```

### Link ↔: how Studio reaches the board

The words before round 2 stayed, apart from four lines.

```text
↔ USB · connected                    # was: USB · live
↔ USB · not connected
◌ USB · connecting
◌ USB · reconnecting
↔ USB · not responding     │ Retry │   ← orange   # was: yellow, an unsaved edit's colour
↔ USB · connection failed  │ Retry │   ← red      # was: Couldn't connect
↔ Wi‑Fi · connected   ☁              # ☁ lit: lightplayer.app reaches it too
↔ Bluetooth · connected
↔ lightplayer.app · connected        # was: Wi‑Fi via lightplayer.app · live
↔ Offline · 2 h
↔ Open in another tab · editor open  ← orange
↔ Taken by another tab               ← orange
↔ Someone else connected   │ Retry │   ← orange
▶ Simulated · running
▶ Simulated · off
```

### Access ○: what you can do

```text
○ Access    edit · USB                     # was: You can edit · USB
○ Access    edit · password                # was: You can edit · Workshop key
○ Access    edit · no password             # quiet: open is the default. was: yellow, "anyone can edit"
○ Access    play only        │ Unlock │    # was: You can play · anyone can play
○ Access    locked                         # primary: Unlock
◌ Access    checking                       # was: Access not known
○ Access    edit                   (dim)   # offline. was: Access not known · when it's back
○ Access    USB only · no password             ← orange   # was: No password here · set one over USB, yellow
                                               # no core offer sets a board's password: the fix is in the details
```

### Firmware ▲: what runs the board

```text
▲ Firmware  v2026.10.16-1                              # was: fw-esp32c6 c688c990b
▲ Firmware  dev c688c99                                # a local build
▲ Firmware  v2026.10.08-9   │ Update │                  # no tone (P16). was: blue, the live family
▲ Firmware  v2026.10.02-1 (pre-OTA) │ Update │           # over USB, once (the USB icon on the button)
▲ Firmware  unknown (pre-2026.07)   │ Install │          # was: Pre-hello firmware; no button when the primary is Install firmware
            # "Update to v2026.10.16-1" does not fit beside the version at the card's width:
            # the button says the verb, and the version it brings is the details' Newest
▲ Firmware  none                                       # primary: Install firmware
◌ Firmware  v2026.10.08-9   updating · 42%             # was: Updating · 1 of 2 · 42%
◌ Firmware  v2026.10.08-9   backing up · 12% │ Cancel │
◌ Firmware  installing · 30%                           # was: Flashing firmware…
▲ Firmware  v2026.10.08-9   crashing     │ Reinstall │   ← red
▲ Firmware  v2026.09.30-2   incomplete   │ Install │   ← orange
▲ Firmware  v2026.10.08-9   confirmation required │ Review │   ← orange   # was: Needs your answer
▲ Firmware  v2026.10.08-9   update interrupted │ Resume │   ← orange      # was: Files waiting
▲ Firmware  v2026.10.16-1   data unreadable           ← orange           # shipped 10-15
▲ Firmware  v2026.10.16-1   update required           ← orange           # its files wait for a firmware that reads their store
▲ Firmware  v2026.10.08-9                      (dim)   # offline
```

### Hardware ■: what the board is

```text
■ Hardware  XIAO ESP32-C6
■ Hardware  ESP32-C6       model not set         # was: ESP32-C6
■ Hardware  unknown                              # was: Board not known
◌ Hardware  identifying                          # was: chip unknown
■ Hardware  XIAO ESP32-S3 Plus                   # "simulated" is said once, on the Link row (P2)
```

### Corner: the picture's state

```text
(nothing)                     live, all well                  # was: ● 899 fps
                              # a quiet ⋯ stays, so the details (the terminal) are one press away
╭─────────╮
│ editing │                   the editor holds it; the picture dims
╰─────────╯
╭───╮
│ ! │                         a problem, only while the panel covers the rows (P2)
╰───╯
(picture dimmed)              stale; the Link row says since when (P13)   # was: 2 h ago
(no picture)                  running nothing                  # was: the last project's picture
```

### Name bar and primary

```text
│ Porch lamp            │ ↔ Connect           │
│ Porch lamp            │ ✓ Done              │   connected
│ Porch lamp            │ ◌                   │   connecting: the Link row says how
│ Porch lamp            │ + Add a project     │   nothing on it
│ Porch lamp            │ ▲ Install firmware  │   # was: Install
│ Porch lamp            │ ○ Unlock            │
│ Porch lamp            │ ▶ Power on          │   simulated, off
│ Porch lamp                                  │   nothing to press: no button   # was: Updating · 42%
│ XIAO ESP32-C6                               │   a new board's name   # was: XIAO ESP32-C6 · Oct 11
```

## Decided (Yona, 2026-10-16: "all leans")

```text
O1  sending stays in place: ◌ Project  Playful Choker  sending · 40%
O2  the project's icon is layers (three stacked chevrons), wherever Studio marks a project
O3  data lost moves to the Project row: Project  none  data lost  │ Restore │ Reset │
    (data unreadable stays on the Firmware row: it is the firmware that refused the store)
O4  an open board is quiet: Access  edit · no password
O5  a new board's name has no date; two boards of one model get a number ("XIAO ESP32-C6 2")
```
