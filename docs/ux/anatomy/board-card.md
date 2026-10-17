# Board card

One board: what it plays, how it's reached, what you can do, what runs it.
Shown on the home page and the Boards tab. Built in core
(`lpa-studio-core/…/board_card/`), drawn by the web
(`lpa-studio-web/…/board_card/`).

Status: **proposed** (round 2 of the card's words). Where a line changes
what ships today, its comment says `today:`.

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
◆ Project   studio                                   # today: studio
◆ Project   studio         2 boards
◆ Project   none                                     # primary: + Add a project
◌ Project   studio         sending · 40%             # bar along the foot
◆ Project   studio         out of date │ Send latest │      # no tone (P16). today: Out of date · studio, orange
◆ Project   studio         error                     ← red   # details: the error
◆ Project   hidden                                   # locked. today: Project not known
◆ Project   studio                         (dim)     # offline. today: Project not known
◌ Project                                            # the board hasn't said yet
```

### Link ↔: how Studio reaches the board

Today's words stay, apart from four lines.

```text
↔ USB · connected                    # today: USB · live
↔ USB · not connected
◌ USB · connecting
◌ USB · reconnecting
↔ USB · not responding     │ Retry │   ← orange   # today: yellow, an unsaved edit's colour
↔ USB · connection failed  │ Retry │   ← red      # today: Couldn't connect
↔ Wi‑Fi · connected   ☁              # ☁ lit: lightplayer.app reaches it too
↔ Bluetooth · connected
↔ lightplayer.app · connected        # today: Wi‑Fi via lightplayer.app · live
↔ Offline · 2 h
↔ Open in another tab · editor open  ← orange
↔ Taken by another tab               ← orange
↔ Someone else connected   │ Retry │   ← orange
▶ Simulated · running
▶ Simulated · off
```

### Access ○: what you can do

```text
○ Access    edit · USB                     # today: You can edit · USB
○ Access    edit · password                # today: You can edit · Workshop key
○ Access    edit · no password             # quiet: open is the default. today: yellow, "anyone can edit"
○ Access    play only        │ Unlock │    # today: You can play · anyone can play
○ Access    locked                         # primary: Unlock
◌ Access    checking                       # today: Access not known
○ Access    edit                   (dim)   # offline. today: Access not known · when it's back
○ Access    USB only · no password │ Set password │   ← orange   # today: No password here · set one over USB, yellow
```

### Firmware ▲: what runs the board

```text
▲ Firmware  v2026.10.16-1                              # today: fw-esp32c6 c688c990b
▲ Firmware  dev c688c99                                # a local build
▲ Firmware  v2026.10.08-9   │ Update to v2026.10.16-1 │   # no tone (P16). today: blue, the live family
▲ Firmware  v2026.10.02-1 (pre-OTA) │ Update to v2026.10.16-1 │   # over USB, once
▲ Firmware  unknown (pre-2026.07)   │ Install v2026.10.16-1 │     # today: Pre-hello firmware
▲ Firmware  none                                       # primary: Install firmware
◌ Firmware  v2026.10.08-9   updating · 42%             # today: Updating · 1 of 2 · 42%
◌ Firmware  v2026.10.08-9   backing up · 12% │ Cancel │
◌ Firmware  installing · 30%                           # today: Flashing firmware…
▲ Firmware  v2026.10.08-9   crashing     │ Reinstall │   ← red
▲ Firmware  v2026.09.30-2   incomplete   │ Install v2026.10.16-1 │   ← orange
▲ Firmware  v2026.10.08-9   confirmation required │ Review │   ← orange   # today: Needs your answer
▲ Firmware  v2026.10.16-1   data lost    │ Restore │ Reset │   ← orange   # today: Its files are gone
▲ Firmware  v2026.10.08-9   update interrupted │ Resume │   ← orange      # today: Files waiting
▲ Firmware  data unreadable                            ← orange           # shipped 10-15
▲ Firmware  v2026.10.08-9                      (dim)   # offline
```

### Hardware ■: what the board is

```text
■ Hardware  XIAO ESP32-C6
■ Hardware  ESP32-C6       model not set         # today: ESP32-C6
■ Hardware  unknown                              # today: Board not known
◌ Hardware  identifying                          # today: chip unknown
■ Hardware  XIAO ESP32-S3 Plus                   # "simulated" is said once, on the Link row (P2)
```

### Corner: the picture's state

```text
(nothing)                     live, all well                  # today: ● 899 fps
╭─────────╮
│ editing │                   the editor holds it; the picture dims
╰─────────╯
╭───╮
│ ! │                         a problem, only while the panel covers the rows (P2)
╰───╯
(picture dimmed)              stale; the Link row says since when (P13)   # today: 2 h ago
(no picture)                  running nothing                  # today: the last project's picture
```

### Name bar and primary

```text
│ Porch lamp            │ ↔ Connect           │
│ Porch lamp            │ ✓ Done              │   connected
│ Porch lamp            │ ◌                   │   connecting: the Link row says how
│ Porch lamp            │ + Add a project     │   nothing on it
│ Porch lamp            │ ▲ Install firmware  │   # today: Install
│ Porch lamp            │ ○ Unlock            │
│ Porch lamp            │ ▶ Power on          │   simulated, off
│ Porch lamp                                  │   nothing to press: no button   # today: Updating · 42%
│ XIAO ESP32-C6                               │   a new board's name   # today: XIAO ESP32-C6 · Oct 11
```

## Open

```text
O1  sending: "Sending ◆ Playful Choker" (Yona's) or "◌ Project  Playful Choker  sending · 40%" (P3)?
O2  the project's icon: ◆ is a stand-in.
O3  data lost / data unreadable: the Firmware row, or the Project row?
O4  an open board (no password): quiet, or orange? (today's yellow is wrong either way, P16)
O5  a new board's name: drop "· Oct 11"?
```
