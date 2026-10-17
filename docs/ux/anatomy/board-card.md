# Board card

One board: what it plays, how it's reached, what you can do, what runs it.
Shown on the home page and the Boards tab. Built in core
(`lpa-studio-core/…/board_card/`), drawn by the web
(`lpa-studio-web/…/board_card/`).

Status: **shipped** (round 3, the card's top as its main surface,
2026-10-17, PR #1176; plan `lp2025/2026-10-17-0315-card-surface-round-3`;
round 2 of the card's words was PR #1159). Where a line changed what
shipped before, its comment says `was:`. ◆ is the layers icon (O2).

## Whole

```text
┌──────────────────────────────────────────┐
│               ( ▲ )               ╭──────┤
│   This board is running old       │  ⋯   │   1 surface · 2 corner
│   firmware. Update the firmware   ╰──────┤
│   to connect and edit.                   │
├────────────────────────────┬─────────────┤
│ XIAO ESP32-C6              │ ▲ Update    │   3 name · 4 primary
│                            │   firmware  │
├────────────────────────────┴─────────────┤
│ ◆ Project   not known                  › │   5 rows
│ ↔ USB · connected                      › │
│ ○ Access    edit · USB                 › │
│ ▲ Firmware  unknown  older LightPlayer │ Update │
│ ■ Hardware  XIAO ESP32-C6              › │
└──────────────────────────────────────────┘
```

```text
1 surface   what matters now: the live picture while the board is well,
            else the state's icon and plain sentences over it.        P18 · was: the picture, black when it showed none
2 corner    a quiet ⋯: the card's details, one press away.            P4 · was: the picture's state, a notice's mark, "Editing"
3 name      the board's own name. rename lives in Hardware's details.
4 primary   the most likely thing to do, or nothing at all.          P19 · was: the one next step
5 rows      one fact each, always in this order.                     P8
```

Yona, 2026-10-17: "the primary surface should always give the most
important information", and "the primary action slot should hold the most
likely action a user wants to take". The surface explains; the primary
gives the thing to do. There are no buttons on the surface.

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

## Surface

The card's top (`board_surface.rs`, `UiBoardSurface`). While the board is
well, its live picture and no words. Otherwise a notice, concept B of the
spike (`spikes/board-card-surface/` on `claude/spike-card-surface`):

```text
┌──────────────────────────────────────────┐
│                 ( ▲ )                    │   the state's icon, in its tone
│   This board is running old firmware.    │   what is going on
│ Update the firmware to connect and edit. │   the primary's button, said aloud
│          ━━━━━━━━━──────  42%            │   work: its bar and percent
└──────────────────────────────────────────┘
```

```text
icon       the row's icon whose state it is, in a circle, in the state's tone:
           orange (attention), red striped (error), plain (neutral, working).
           the tone is the icon's alone: the sentences stay plain.
first      what is going on with the board. one plain sentence.
second     the primary's button said aloud: its verb and object, then what it
           gets you. said only while that button is the primary; a refusal
           ("Firmware updates need USB.") in its place when the link refuses it;
           and when the fix isn't offered here, why and what to do instead
           ("Connect over USB or Bluetooth to install it.", "Studio doesn't have
           firmware for this board."). no dead ends (P7).
progress   the work's bar and percent; the spinner in the icon's place when
           it has no number. the percent is the one the working row carries
           and says: an update is one number, the new firmware 0–49 %, its
           last part 50–100 %, here, on the row and in the editor alike.
picture    dims under the notice, fully covered: one dim, the cover's (never a
           stale picture's own as well). no picture: the notice on the dark.
```

Every state, first match wins. `↻` Retry, `↔` Connect, `◌` the spinner:

```text
state                       surface                                                        primary
connecting                  Connecting to this board over USB.  + spinner                   ◌
reconnecting                Reconnecting to this board over USB.  + spinner                 ✓ Done
moving here (take-over)     Moving this board to this tab.  + spinner                       ◌
confirmation required       This update is waiting for your confirmation.                  (none: its details rise)
updating                    This board is updating its firmware.  + bar                     (none)
installing                  Installing the firmware on this board.  + bar                   (none)
erasing                     Erasing this board.  + bar                                      (none)
sending                     Sending the project “studio” to this board.  + bar              (none)
removing                    Removing the project from this board.  + bar                    (none)
identifying                 Identifying this board.  + spinner                              (none)
move failed                 Studio couldn't move this board to this tab.
                            Connect here to try again.                                     ↔ Connect here
taken by another tab        Another tab took this board.
                            Connect here to move it back to this tab.                      ↔ Connect here
busy in another tab         This board is busy in another tab.
                            Connect here when it's done.                                   (none)
another tab                 This board is open in another tab.
                            Connect here to move it to this tab.                           ↔ Connect here
someone else                Someone else is connected to this board.
                            Retry when they're done.                                       ↻ Retry
wifi failed                 Studio couldn't reach this board over Wi‑Fi.
                            Make sure this computer is on the same Wi‑Fi, then retry.      ↻ Retry
lightplayer.app failed      Studio couldn't reach this board through lightplayer.app.
                            Retry to connect.                                              ↻ Retry
connect failed              Studio couldn't connect to this board over USB.
                            Retry to connect.                                              ↻ Retry
not answering               This board stopped responding over USB.
                            Retry to reconnect.                                            ↻ Retry
sim off                     This simulated board is off.                                   ▶ Power on
offline, another way        This board isn't plugged in.
  reaches it                Connect over Wi‑Fi to use it.                                  ↔ Connect
offline                     This board is offline.  It was last seen 2 hours ago.          (none; ↔ Connect by its cable when it can)
disconnected (port closed)  Studio isn't connected to this board.  Connect over USB to use it.   ↔ Connect
files need newer firmware   This board's files need newer firmware.
                            Update the firmware to read them.                              ▲ Update firmware
data unreadable             This board's firmware can't read its files.
                            The files are kept on the board.                               (none: no click is safe)
data lost                   This board's files were lost.
                            Restore them from a backup, or reset the board.                ◆ Restore (else Reset)
update interrupted          This board's firmware update was interrupted.
                            Resume the update to finish it.                                ▲ Resume update
send failed                 Studio couldn't send the project to this board.
                            Retry to send it again.                                        ↻ Retry
update failed               This board's firmware update didn't finish.  Retry to update it.   ↻ Retry
install failed              Installing the firmware on this board didn't finish.
                            Retry to install it.                                           ↻ Retry
erase failed                Erasing this board didn't finish.  Retry to erase it.          ↻ Retry
removal failed              Removing the project from this board didn't finish.
                            Retry to remove it.                                            ↻ Retry
crashing                    This board's firmware keeps crashing.
                            Reinstall the firmware to fix it.                              ▲ Reinstall firmware
a version Studio can't get  This board needs a firmware version Studio can't get.
                            Install the firmware to use it.                                ▲ Install firmware
                                                                     (Studio's own: one press, or the version its list picks)
incomplete (core, no        Part of this board's firmware is missing.
  engine)                   Install the firmware to use it.                                ▲ Install firmware
old firmware (an older      This board is running old firmware.
  LightPlayer)              Update the firmware to connect and edit.                       ▲ Update firmware
from before the hello       This board is running old firmware.
                            Install the firmware to connect and edit.                      ▲ Install firmware
no firmware                 This board doesn't have LightPlayer firmware.
                            Install the firmware to use it.                                ▲ Install firmware
download mode               This board is waiting in download mode.
                            Install the firmware to use it.                                ▲ Install firmware
says nothing                This board doesn't respond.  Install the firmware to use it.   ▲ Install firmware
locked                      This board is locked.  Unlock it to connect.                   ○ Unlock
a shader error              This board's project has an error.  Edit the project to fix it.   ✎ Edit project
no project                  This board has no project yet.                                 + Add a project
in the editor               This board is open in the editor.                              ✓ Done
connected                   the live picture, no words (the rows become the panel)          ✓ Done
picture too large           This board's picture is too large to show over Bluetooth.      ↔ Connect
all well                    the live picture, no words                                     ↔ Connect
```

One update over USB first (a pre-update board) is no notice: the board
plays and connects, and the Firmware row's own Update carries it.

A failed row's state holds only while its row offers the verb again; once
it doesn't, the board's own state says what comes next (an update that
found no copy of the engine reads "a version Studio can't get", Install
firmware). When a firmware fix isn't offered here the second sentence says
why, read off the board's update story:

```text
over Wi‑Fi, before it updates there    Connect over USB or Bluetooth to install it.
through lightplayer.app, the same      Connect nearby to install it.
Studio's build can't go over the air   Connect over USB to install it.
a version Studio can't get             Connect this computer to the internet to get it.
files need newer firmware              Studio doesn't have firmware new enough to read them.
else                                   Studio doesn't have firmware for this board.
```

**The rows go quiet.** The row whose problem the surface says keeps its
state word and its own button and loses its orange or red: the surface is
the one loud place. The primary may repeat that row's button; it says verb
and object ("Update firmware") where the row says the verb alone
("Update"), because the row already names its object (P12).

## The row

```text
│ ◆ Project   studio        out of date │ Send latest │
  │ │         │             │             │
  │ │         │             │             action   verb (+ object). fixes the state. at most one.
  │ │         │             state         only when not normal. its tone says how bad.
  │ │         │                           gives way first: cut, then gone, before the name is cut.
  │ │         name          the thing itself. here in every state. keeps its width.   # was: cut first
  │ kind                    muted. teaches what the row is about.
  icon                      the kind's one icon, the same everywhere in the app.
```

```text
›          no action: the row opens its details (everything the row leaves out)
tone       none · orange: needs you · red (striped): broken · blue: live, the lit ☁ only
           (ui.md's status families; the card adds none, P16).
           quiet while the surface says the row's problem.   # was: the row was the loud place
working    ◌ takes the icon's place; progress runs along the row's foot ━━━━────
stale      the row dims; its words don't change
```

The Link row is the one exception: the link's own icon and name are its
kind (`USB`, `Wi‑Fi`, `Bluetooth`, `lightplayer.app`), as a version
number is.

Short of room, a row gives way in one order: its state first (cut at its
end, then gone), then, on a narrow card, its kind word (the icon stays),
and its name only when it alone is wider than the row:

```text
▲ Firmware  v2026.10.03-1  crashing  │ Reinstall │   wide
▲ Firmware  v2026.10.03-1            │ Reinstall │   the state gone   # was: v crashing
▲ v2026.10.03-1                      │ Reinstall │   a narrow card
```

## Each row

### Project ◆: what the board plays

```text
◆ Project   studio                                   # was: studio, with no kind word
◆ Project   studio         2 boards
◆ Project   none                                     # it said it runs nothing, or has no LightPlayer. primary: + Add a project
◆ Project   not known                                # an older LightPlayer, a board from before the hello, a core alone. was: none (the desk C6, 10-17)
◌ Project   studio         sending · 40%             # bar along the foot
◆ Project   studio         out of date │ Send latest │      # no tone (P16). was: Out of date · studio, orange
◆ Project   studio         error                     # red; quiet while the surface says it. details: the error
◆ Project   none           data lost  │ Restore │ Reset │   # O3; quiet while the surface says it
◆ Project   hidden                                   # locked. was: Project not known
◆ Project   studio                         (dim)     # offline. was: Project not known
◆ Project   not known                      (dim)     # offline, nothing known of it
◌ Project                                            # a LightPlayer that hasn't said yet
```

### Link ↔: how Studio reaches the board

```text
↔ USB · connected                    # was: USB · live
↔ USB · not connected
◌ USB · connecting
◌ USB · reconnecting
↔ USB · not responding     │ Retry │   # orange; quiet while the surface says it
↔ USB · connection failed  │ Retry │   # red; quiet while the surface says it
↔ Wi‑Fi · connected   ☁              # ☁ lit: lightplayer.app reaches it too
↔ Bluetooth · connected
↔ lightplayer.app · connected        # was: Wi‑Fi via lightplayer.app · live
↔ Offline · 2 hours ago              # the surface's sentence says the age the same way. was: Offline · 2 h
↔ Wi‑Fi · not connected              # unplugged, and Wi‑Fi reaches it (lightplayer.app · not connected the same). was: Offline · 2 h
↔ Open in another tab · editor open  # quiet: the surface says it. was: orange
↔ Taken by another tab               # quiet: the surface says it. was: orange
↔ Someone else connected   │ Retry │   # quiet: the surface says it. was: orange
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
▲ Firmware  unknown   older LightPlayer │ Update │       # one click: it keeps the project and files. primary: Update firmware. was: red, two clicks
▲ Firmware  unknown (pre-2026.07)   │ Install │          # was: Pre-hello firmware; no button when the primary is Install firmware
            # "Update to v2026.10.16-1" does not fit beside the version at the card's width:
            # the button says the verb, and the version it brings is the details' Newest
▲ Firmware  none                                       # primary: Install firmware
◌ Firmware  v2026.10.08-9   updating · 42%             # was: Updating · 1 of 2 · 42%
◌ Firmware  v2026.10.08-9   backing up · 12% │ Cancel │
◌ Firmware  installing · 30%                           # was: Flashing firmware…
▲ Firmware  v2026.10.08-9   crashing     │ Reinstall │   # red; quiet while the surface says it
▲ Firmware  v2026.09.30-2   incomplete   │ Install │     # orange; quiet while the surface says it
▲ Firmware  v2026.10.08-9   confirmation required │ Review │   # was: Needs your answer
▲ Firmware  v2026.10.08-9   update interrupted │ Resume │      # was: Files waiting
▲ Firmware  v2026.10.16-1   data unreadable                    # shipped 10-15
▲ Firmware  v2026.10.16-1   update required                    # its files wait for a firmware that reads their store
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

### Corner: the card's details

```text
╭───╮
│ ⋯ │                         every state: the details (every notice, the
╰───╯                         picture's facts, the terminal) one press away
                              # was: (nothing) live · "editing" while the editor held it ·
                              #      "!" while the panel covered the rows. the surface says those now.
```

### Name bar and primary

```text
│ Porch lamp            │ ↔ Connect           │
│ Porch lamp            │ ✓ Done              │   connected, reconnecting, in the editor
│ Porch lamp            │ ◌                   │   connecting, moving here from another tab
│ Porch lamp            │ ▲ Update firmware   │   old firmware; files that need newer firmware   # was: no button (the desk C6)
│ Porch lamp            │ ▲ Install firmware  │   no firmware, incomplete, a version Studio can't get   # was: Install
│ Porch lamp            │ ▲ Reinstall firmware│   crashing   # was: Connect
│ Porch lamp            │ ▲ Resume update     │   an update stopped half-way
│ Porch lamp            │ ◆ Restore           │   its files lost (Reset when there is no backup)   # was: Add a project
│ Porch lamp            │ ↻ Retry             │   not answering, a connect that failed or was turned away, failed work   # was: Connect, or nothing
│ Porch lamp            │ ↔ Connect here      │   another tab holds it   # was: Connect
│ Porch lamp            │ ✎ Edit project      │   a shader error
│ Porch lamp            │ + Add a project     │   nothing on it
│ Porch lamp            │ ○ Unlock            │
│ Porch lamp            │ ▶ Power on          │   simulated, off
│ Porch lamp                                  │   nothing to press: no button; the surface says why
│ XIAO ESP32-C6                               │   a new board's name   # was: XIAO ESP32-C6 · Oct 11
│ XIAO ESP32-…  │ ▲ Update │                      a narrow card   # was: XIAO ES… │ Update firmware
```

The name keeps about twelve characters. On a card too narrow for that
beside the whole primary, the primary says its verb alone ("Update",
"Install", "Reinstall", "Resume", "Edit", "Add", "Connect"): the surface's
sentence already names the object. The button's text stays whole for a
screen reader.

## Decided

Yona, 2026-10-16 ("all leans"):

```text
O1  sending stays in place: ◌ Project  Playful Choker  sending · 40%
O2  the project's icon is layers (three stacked chevrons), wherever Studio marks a project
O3  data lost moves to the Project row: Project  none  data lost  │ Restore │ Reset │
    (data unreadable stays on the Firmware row: it is the firmware that refused the store)
O4  an open board is quiet: Access  edit · no password
O5  a new board's name has no date; two boards of one model get a number ("XIAO ESP32-C6 2")
```

Round 3, 2026-10-17:

```text
R1  the card's top is its main surface, never black; concept B, the notice as an object (Yona)
R2  the copy rule: the second sentence is the primary's button said aloud (Yona)
R3  the primary holds the most likely action, even when it repeats a row's button (Yona)
R4  the rows go quiet while the surface says their problem        (director's call, pending Yona's veto)
R5  B covers a picture: it dims under the notice, one dim          (director's call, pending Yona's veto)
R6  the tone is the icon's alone; the sentences stay plain         (director's call, pending Yona's veto)
R7  offline, another way reaches it: the Link row names that way  (director's call, pending Yona's veto)
R8  a project's name in running text is quoted                     (director's call, pending Yona's veto)
R9  a row's value keeps its width; its state gives way first: cut, then gone   (director's review)
    (on a narrow card the kind word gives way next, its icon staying: pending review)
R10 one update, one number: the surface reads the working row's percent       (director's review)
R11 no dead ends: a fix that isn't offered says why and what to do instead    (director's review)
R12 a narrow name bar keeps the name's twelve characters; the primary says its verb alone   (director's review)
```
