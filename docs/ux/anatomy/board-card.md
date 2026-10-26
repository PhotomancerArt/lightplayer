# Board card

One board: what it plays, how it's reached, what you can do, what runs it.
Shown on the home page and the Boards tab. Built in core
(`lpa-studio-core/…/board_card/`), drawn by the web
(`lpa-studio-web/…/board_card/`).

Status: **shipped** (the eight rows as a new board's setup list,
2026-10-19, plan `lp2025/2026-10-19-1350-board-setup-rows`, spike
`spikes/install-network-setup`; round 3, the card's top as its main
surface, was PR #1176; round 2 of the card's words was PR #1159). Where a line changed what
shipped before, its comment says `was:`. ◆ is the layers icon (O2).

## Whole

```text
┌──────────────────────────────────────────┐
│               ( ▲ )               ╭──────┤
│   This board is running old       │  ⋯   │   1 surface · 2 corner
│   firmware. Update the firmware   ╰──────┤
│   to connect and edit.                   │
├────────────────────────────┬─────────────┤
│ XIAO ESP32-C6 ✎            │ ▲ Update    │   3 name · 4 primary
│                            │   firmware  │
├────────────────────────────┴─────────────┤
│ ◆ Project          not known           › │   5 rows
│ ⏚ USB              connected           › │
│ ≋ Wi‑Fi            Cedar Lane          › │
│ ᛒ Bluetooth        LP-3f2a             › │
│ ☁ lightplayer.app  river@example.com   › │
│ ○ Access           edit · USB          › │
│ ▲ Firmware         unknown  older LightPlayer │ Update │
│ ■ Hardware         XIAO ESP32-C6       › │
└──────────────────────────────────────────┘
```

```text
1 surface   what matters now: the live picture while the board is well,
            else the state's icon and plain sentences over it.        P18 · was: the picture, black when it showed none
2 corner    a quiet ⋯: the card's details, one press away.            P4 · was: the picture's state, a notice's mark, "Editing"
3 name      the board's own name, renamed where it is: a faint ✎.   # was: rename in Hardware's details
4 primary   the most likely thing to do, or nothing at all.          P19 · was: the one next step
5 rows      one fact each, always in this order: for a new board,    P8
            its setup list, the rows with something left to do lit.  # was: five rows, one Link row
```

Yona, 2026-10-17: "the primary surface should always give the most
important information", and "the primary action slot should hold the most
likely action a user wants to take". The surface explains; the primary
gives the thing to do. There are no buttons on the surface.

Connected, the rows become the board's whole panel and the primary becomes
Done. The card grows once to hold it, and shrinks back on Done:

```text
├────────────────────────────┬─────────────┤
│ Playful choker             │ ✓ Done      │
├────────────────────────────┴─────────────┤
│    ◎            ◎            ◎           │   knobs and toggles, as many to a row
│ BRIGHTNESS    SPEED        SCALE         │   as the card is wide; the master first
│    128          1×          1.6          │   # was: the master fader across the card
│ PALETTE                    ↻ 4 · 20 s    │   a palette across the card
│ ▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬ │
│ PLASMA 1 ─────────────────────────────── │   a nested group under a quiet heading
│    ◎ Hue        ◎ Width                  │
├────────────────────────────────┬─────────┤
│                                │ ✎ Edit  │   the foot: Edit, or its lock
└────────────────────────────────┴─────────┘   # was: All controls · 3 more │ Edit
```

```text
Speed      the clock, one knob: log ¼×–8×, 1× at the top, the octave
           stops marked. the clock's own card keeps the whole tape.     # was: the tape, on the play page
Brightness a knob on the panel; the fixture's own card keeps its fader. # was: a fader across the card
```

Nothing on the card links away: every control is here, and the play page
is an address (phones, shared links, the editor's Play), not a row.

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
           it has no number. the one bar and the one number the work has: the
           row doing it is carried (its verb and its spinner, no fill, no
           number). an update is one number, the new firmware 0–49 %, its
           last part 50–100 %, here and in the editor alike, and it only
           moves forward.
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
not answering (it said      This board stopped responding over USB.
  hello, now says nothing)  Retry to reconnect.                                            ↻ Retry
sim off                     This simulated board is off.                                   ▶ Power on
online at lightplayer.app   its picture from lightplayer.app, no words                      ☁ Connect (≋ Connect while it shares your network)
  (no link here)            (no picture yet, or relay protocol 1: the dark, no words)
offline, another way        This board isn't plugged in.
  reaches it                Connect over Wi‑Fi to use it.                                  ↔ Connect
offline                     This board is offline.  It was last seen 2 hours ago.          (none; ↔ Connect by its cable when it can)
  (on your account too:     the same words, over the last picture lightplayer.app kept,   (none: nothing reaches it)
   offline everywhere)      dimmed; "last seen" is the newest any computer saw
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
no firmware (blank, other  This board doesn't have LightPlayer firmware.
  firmware, or erased here) Install the firmware to use it.                                ▲ Install firmware
                                                                     # was: stopped responding, Retry (a board Studio had just erased, Yona's C6 10-19)
download mode               This board is waiting in download mode.
                            Install the firmware to use it.                                ▲ Install firmware
says nothing (it never      This board doesn't respond.  Install the firmware to use it.   ▲ Install firmware
  said hello)                                                        # was: not answering, Retry (a new board, lightplayer.app 10-18)
locked                      This board is locked.  Unlock it to connect.                   ○ Unlock
a shader error              This board's project has an error.  Edit the project to fix it.   ✎ Edit project
no project                  This board has no project yet.                                 + Add a project
in the editor               This board is open in the editor.                              ✓ Done
connected                   the live picture, no words (the rows become the panel)          ✓ Done
picture too large           This board's picture is too large to show over Bluetooth.      ↔ Connect
all well                    the live picture, no words                                     ↔ Connect
```

**Online at lightplayer.app** is a board of the signed-in account that the
account's list (`ListAccountBoards`) says is online, and that no link of this tab
reaches: one this browser knows, or one it never met (set up on a phone,
say). It is well, so the surface is its picture and no words, the picture
lightplayer.app last had (`BoardPictures`), drawn one row of lamps per
output and not dimmed: it is current. Its card stands in Online boards; its
lightplayer.app row says `river@example.com · online`; Connect goes through
lightplayer.app. A board with a USB, Bluetooth or LAN link here keeps that
link and its own surface: the list only lets its lightplayer.app row say
online.

```text
┌──────────────────────────────────────────┐
│     ● ● ● ● ● ● ● ● ● ● ● ● ● ●     ╭────┤
│     ● ● ● ● ● ● ● ●                 │ ⋯  │   its picture, from lightplayer.app
├────────────────────────────┬─────────┴────┤
│ Fabric Test ✎              │ ☁ Connect    │
├────────────────────────────┴──────────────┤
│ ◆ Project          rainbow              › │   what it told lightplayer.app
│ ⏚ USB              —                    › │
│ ≋ Wi‑Fi            —                    › │
│ ᛒ Bluetooth        —                    › │
│ ☁ lightplayer.app  river@example.com  online │
│ ○ Access           —                    › │
│ ▲ Firmware         2026.10.19-1         › │
│ ■ Hardware         —                    › │
└──────────────────────────────────────────┘
```

```text
bad:  the board set up on a phone shows nowhere on the computer   # Yona, 10-19 ("Fabric Test"): Studio only
                                                                  #   offered the relay to boards it had met
```

**Offline, on your account** is a board on the signed-in account's list
(`ListAccountBoards`, the account's board list) that is not online at
lightplayer.app and that no link of this tab reaches: one this browser
remembers, or one it never met (the list follows the account to every
computer). It is the offline card: the surface says "This board is offline.
It was last seen 2 hours ago.", where "last seen" is the newest any link of
any computer saw it; under it, dimmed, the last picture lightplayer.app kept
of it (kept across a deploy, written behind by the relay), or the dark when
there is none. Its rows say their last known values, dimmed: Project and
Firmware as the board last reported them, Hardware its model when a Studio
reported one, lightplayer.app `river@example.com · 2 h ago`.
Nothing reaches it, so there is no Connect; a board it never met offers
Remove from your account and nothing else. A board this browser remembers
takes the record's facts only where the record saw it more recently, and
keeps its own saved picture.

```text
┌──────────────────────────────────────────┐
│   ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░       ╭────┤   its last picture at lightplayer.app,
│              ( ↔ )                  │ ⋯  │     dimmed under the notice
│       This board is offline.        ╰────┤
│   It was last seen 2 hours ago.          │
├──────────────────────────────────────────┤
│ Porch ✎                                  │   no primary: nothing reaches it
├──────────────────────────────────────────┤
│ ◆ Project          rocaille             › │   last known, dimmed
│ ⏚ USB              —                    › │
│ ≋ Wi‑Fi            —                    › │
│ ᛒ Bluetooth        —                    › │
│ ☁ lightplayer.app  river@example.com  2 h ago │   dimmed: when it was last seen
│ ○ Access           —                    › │
│ ▲ Firmware         2026.10.17-2         › │   last known, dimmed
│ ■ Hardware         XIAO ESP32-C6        › │   the model a Studio reported
└──────────────────────────────────────────┘
```

As walked on a profile that never met the board (`just walk-wifi-emu
studio-relay-account`, T10, 2026-10-21), two rows read otherwise than the
drawing above, which is the remembered board's: the name has no ✎ (a board
never met here offers no rename; its name is the one the board reports, D5),
and Access reads `unknown` rather than `—`. USB reads `not connected`, as on
every card with no cable. A board that never reached lightplayer.app (one
joined over USB with no Wi‑Fi) has no picture: the dark, under the notice.

```text
bad:  the board is on the phone's account, and the computer shows nothing while it is unplugged   # M2's reason to be
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

**Connect takes the best road, in one press.** A board no link of this tab
reaches has one Connect, and it tries the roads in order: Wi‑Fi on your
own network, then lightplayer.app, then its cable (a click). The Wi‑Fi
address is the one lightplayer.app says the board gave, while its list
says the board shares this computer's network, else the one this browser
remembers; while the list says the board is on another network, Wi‑Fi is
skipped. A page whose browser never opens a socket to the LAN (an https
page outside Chrome) skips it too. The primary's glyph is the first
road's, not the board's last link: a board online at lightplayer.app that
shares your network reads `≋ Connect`, not `☁ Connect`.

```text
┌──────────────────────────────────────────┐
│     ● ● ● ● ● ● ● ● ● ● ● ● ● ●     ╭────┤
│     ● ● ● ● ● ● ● ●                 │ ⋯  │   its picture, from lightplayer.app
├────────────────────────────┬─────────┴────┤
│ Fabric Test ✎              │ ≋ Connect    │   Wi‑Fi first: it shares your network
├────────────────────────────┴──────────────┤
│ ≋ Wi‑Fi            —                    › │   its details: Connect over Wi‑Fi
│ ☁ lightplayer.app  river@example.com  online │   its details: Connect through lightplayer.app
└──────────────────────────────────────────┘
```

While a road runs, the surface says the road in flight, as the connecting
state already does: "Connecting to this board over Wi‑Fi.", and when Wi‑Fi
doesn't answer, the same press goes on and the line becomes "Connecting to
this board through lightplayer.app." There is no new state. The Wi‑Fi
failure is not shown when lightplayer.app then answers: one line in the
log says it (`Couldn't reach Fabric Test over Wi‑Fi; trying
lightplayer.app.`). When both fail, the card shows lightplayer.app's words
and its Retry. "Connect over Wi‑Fi" or "Connect through lightplayer.app"
pressed in a row's details is that road alone. A board this tab already
reaches (USB, Bluetooth, Wi‑Fi or lightplayer.app) has no road: Connect
opens its session on that link.

The connecting state lasts from the press until the board says hello, on
the card that was pressed. The socket answering (through lightplayer.app,
the secure link coming up) is half-way: the board is still being
identified, and the card keeps its spinner rather than going back to its
Connect. A board never met here does not trade its card for a separate
"Identifying" one meanwhile. A board that never says hello gives up after a
minute, in a held Connect's words. A second press finds the button waiting
and dials nothing.

```text
do:   Connecting to this board through lightplayer.app.  ◌   # press → hello, one state
bad:  (its picture, no words)               │ ☁ Connect │   # the relay answered, the board
                                                            #   has not said hello: it reads
                                                            #   as a press that was lost
```

```text
do:   │ Fabric Test                │ ≋ Connect │   # one press: Wi‑Fi, else lightplayer.app
bad:  │ Fabric Test                │ ☁ Connect │   # on your own network, through the cloud:
                                                   #   the slow road when the fast one is there
bad:  Studio couldn't reach this board over Wi‑Fi.   # a second press for what Studio could try itself
      Make sure this computer is on the same Wi‑Fi, then retry.      ↻ Retry
```

**A session through lightplayer.app moves onto Wi‑Fi by itself**, once per
page load, when the list says the board shares your network: the card's
lightplayer.app row stops saying `connected` and its Wi‑Fi row starts, and
nothing else changes. The editor stays as it was, with no "Reconnecting…".
If Wi‑Fi doesn't answer, the board stays on lightplayer.app and the log says
why in one line; if Wi‑Fi drops later, the board goes back through
lightplayer.app within about five seconds, the way any dropped link does.
It never moves while the board is busy (an update, a project being sent).
Watching a board on its card never opens a session: only Connect and Edit
do.

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
carried    the surface says the row's work: ◌ in the icon's place, the verb alone
           as the state ("updating"), no fill, no number: the surface has them.   # was: the bar and the number twice
waiting    a check nobody asked for (access, a project not said yet): a dash and a
           still word, never a spinner (P4)
name       the value, plain in every state, "—" while it isn't known yet; never bold for work
stale      the row dims; its words don't change
```

The four link rows' kind is the link's own name (`USB`, `Wi‑Fi`,
`Bluetooth`, `lightplayer.app`), drawn a step brighter than the other kinds:
the link is the thing, as a version number is. Each says what its link
reaches or how it stands, and "connected" beside a network, a name or an
account marks the link this tab is using. What this tab is doing about
reaching the board (connecting, reconnecting, a Connect that failed,
another tab or a stranger holding it, a board that stopped answering) is
said on the row of the link it is about.   # was: one Link row, "USB · connected"

**A row with something left to do is lit** (a new board's setup list): the
whole row on a lighter flat ground, its icon, kind and name at full
strength, its button on the same ground with no fill of its own. No
gradient, no dot. Never on a toned row (orange and red keep their tone),
never on a disabled one, never on a row whose fix is another row's
(lightplayer.app waiting on Wi‑Fi), never while it works.

```text
│ ◆ Project          none                 │ + Add │     lit: something to do, and the button that does it
│ ≋ Wi‑Fi            not connected       │ Set up │    the state says the fact; the button says the fix
│ ☁ lightplayer.app  not connected      │ Sign in │
│ ▲ Firmware         none              │ Install │
```

```text
bad:  Wi‑Fi   not set up   │ Set up │      # Yona 10-19: "isn't dry": the state is the fact, the button the fix
bad:  a gradient across the lit row, or a dot  # Yona 10-19: "the lit row alone"
```

**Hushing a row is switching it off, at the top of its details.** Wi‑Fi,
Bluetooth and lightplayer.app each open on one line, drawn alike, as macOS
draws the top of its Wi‑Fi and Bluetooth menus: the row's icon and name,
and the board's own switch at the line's end (on in the good tone). Each is
the board's own setting, so the choice holds across browsers and is true.
Off, the row says `disabled`, plain, with no button, and the same switch
turns it back on. Where this link may not flip it the switch is dimmed, and
a refusal says why under the line ("Bluetooth can only be turned off by
USB…"); its hover says what a flip does, in the offer's words (Bluetooth's
restarts the board).

```text
ᛒ Bluetooth        LP-3f2a                 › │   the row, which opened them
──────────────────────────────────────────────
ᛒ Bluetooth                         (●━━)    │   the switch line: icon, name, the board's switch
  A phone or laptop nearby can connect to it by this name.
  NAME   LP-3f2a
```

```text
bad:  ACTIONS · Disable Wi‑Fi under the list; Disable lightplayer.app beside Sign in; a red Disable Bluetooth   # Yona 10-19: "rendered differently in each of the three sections"
do:   one switch line atop each: icon, name, switch — the same in all three
```

Short of room, a row's state gives way first (cut at its end, then gone).
The kind column stays, so every card in a grid reads alike (P8, P1); where
the row is still narrower than its name and its button (four to a row, a
phone's two), the name is cut at its end and the details hold it whole:

```text
▲ Firmware  v2026.10.03-1  crashing  │ Reinstall │   wide
▲ Firmware  v2026.10.03-1            │ Reinstall │   the state gone   # was: v crashing
▲ Firmware  v2026.10.0…              │ Reinstall │   still narrower: the name cut at its end
```

## Each row

### Project ◆: what the board plays

```text
◆ Project   studio                                   # was: studio, with no kind word
◆ Project   studio         2 boards
◆ Project   none          │ Add │                    # it said it runs nothing: lit, the primary's pick. primary: + Add a project
◆ Project   none                                     # no LightPlayer on it: Install firmware is the primary
◆ Project   not known                                # an older LightPlayer, a board from before the hello, a core alone. was: none (the desk C6, 10-17)
◌ Project   studio         sending                   # carried: the surface has the bar. was: sending · 40%, a bar along the foot
◆ Project   studio         out of date │ Send latest │      # no tone (P16). was: Out of date · studio, orange
◆ Project   studio         error                     # red; quiet while the surface says it. details: the error
◆ Project   none           data lost  │ Restore │ Reset │   # O3; quiet while the surface says it
◆ Project   hidden                                   # locked. was: Project not known
◆ Project   studio                         (dim)     # offline. was: Project not known
◆ Project   not known                      (dim)     # offline, nothing known of it
◆ Project   —                                        # a LightPlayer that hasn't said yet: still. was: ◌ Project
```

### USB ⏚: the board's cable

```text
⏚ USB              connected                 # this tab reaches it by its cable
⏚ USB              not connected             # port closed, unplugged, or reached another way. quiet: normal for a board on a house
⏚ USB              not connected     (dim)   # offline; the surface says since when
◌ USB              —   connecting      # the name is never empty (U4). was: USB connecting, in the name's place
◌ USB              —   reconnecting
⏚ USB              not responding    │ Retry │   # a board that said hello went quiet. orange; quiet while the surface says it
⏚ USB              connection failed │ Retry │   # red; quiet while the surface says it
⏚ USB              Open in another tab  editor open   # quiet: the surface says it
⏚ USB              Taken by another tab
▶ Simulated        running                   # a stand-in keeps the row and says what it is
▶ Emulated         off
```

Its details, while it is the link in use: how it reaches the board (now,
last heard, the board's own words), Identify again, Connect, Disconnect,
and its link counters.

### Wi‑Fi ≋: the networks the board joins

```text
≋ Wi‑Fi            —                         # no firmware yet, not answered, a stand-in
≋ Wi‑Fi            not connected    │ Set up │   # nothing saved: lit. Set up opens the details at the list
◌ Wi‑Fi            Cedar Lane   joining
≋ Wi‑Fi            Cedar Lane                # joined
≋ Wi‑Fi            Cedar Lane   connected    # joined, and this tab's link
≋ Wi‑Fi            Cedar Lane   wrong password  │ Change │   # orange
≋ Wi‑Fi            Cedar Lane   not connected    # saved, not in range: it joins when it is
≋ Wi‑Fi            2 networks   saved        # a firmware that saves networks and can't join yet
≋ Wi‑Fi            disabled                  # hushed: no button
≋ Wi‑Fi            Cedar Lane        (dim)   # offline: the network it last said
```

Its details: the switch line (Wi‑Fi and the board's switch), then the
Wi‑Fi panel (its networks, adding one), and Connect over Wi‑Fi where the
primary doesn't already offer it. Off: the switch stands off over "Wi‑Fi is
disabled on this board. It reaches Studio over USB and Bluetooth only."
The panel draws no heading or switches of its own on the card: the switch
line is both, and lightplayer.app's switch is its own row's.

### Bluetooth ᛒ: the board's own radio

```text
ᛒ Bluetooth        —                         # nothing read yet, a stand-in
ᛒ Bluetooth        LP-3f2a                   # the name a phone sees: LP-<project>, else LP-<MAC's last two bytes>
ᛒ Bluetooth        LP-3f2a   connected       # this tab's link
ᛒ Bluetooth        on                        # on, but what it runs is not known (offline, or before its first report), so neither is the name
ᛒ Bluetooth        disabled
```

Its details: the switch line (Bluetooth and the board's switch: a flip
restarts the board to apply it, so it is Undoable, and off is refused over
Bluetooth, the link you are on), then the name, "A phone or laptop nearby
can connect to it by this name.", and on an open board "No password yet:
anyone nearby or on the same Wi‑Fi can connect and edit it." Off: the switch
stands off over "Bluetooth is disabled on this board: nothing nearby can
connect by it."

### lightplayer.app ☁: the board through the cloud

```text
☁ lightplayer.app  —                         # no firmware, not answered, no relay on this firmware
☁ lightplayer.app  not connected   │ Sign in │   # signed out: lit
☁ lightplayer.app  river@example.com   no Wi‑Fi      # signed in, the board not on Wi‑Fi (nothing saved, none joined, or off;
                                                     #   the details say which): its fix is Wi‑Fi's. not lit
☁ lightplayer.app  river@example.com   not added     # the board doesn't hold the account's key
☁ lightplayer.app  river@example.com   online        # reachable from any browser signed in to it; with no
                                                    #   link here, the account's list says it (ListAccountBoards)
☁ lightplayer.app  river@example.com   2 h ago       # on the account's list, offline everywhere, no link
                                                    #   here: when any computer last saw it. dimmed
☁ lightplayer.app  river@example.com   list full     # the board holds the account's key and the account's list
                                                    #   holds 64 boards: it is not on it. orange; the details
                                                    #   say "Your account holds 64 boards. Remove one to add
                                                    #   this one." once, with no button of its own
☁ lightplayer.app  river@example.com   connected     # this tab's link
☁ lightplayer.app  river@example.com   connecting
☁ lightplayer.app  river@example.com   can't reach   # joined, and it didn't answer (or refused it). orange
☁ lightplayer.app  disabled                  # hushed, for privacy
```

Its details open on the switch line (lightplayer.app and the board's Cloud
relay switch). Signed out, then "Sign in to reach this board from any
browser, through lightplayer.app." with Sign in (the person's own click: it
leaves the page), and "Nothing goes to lightplayer.app until you sign in."
Signed in: the account, how it stands now, and "Lets lightplayer.app reach
this board through the cloud.", or, off Wi‑Fi, why it can't ("This board
isn't set up on Wi‑Fi, so lightplayer.app can't reach it. Set up Wi‑Fi to
reach it from any browser."). Offline, the row keeps the board's last
Wi‑Fi facts, dimmed, but never says online, connecting or can't reach. Off: the switch stands off over
"lightplayer.app is disabled for this board: it never connects to the
cloud."

```text
bad:  USB · connected  ☁̸        # the struck cloud: Yona's desk, 10-19. lightplayer.app's row says it now
bad:  ☁ lightplayer.app  Yona's account     # Yona's phone, 10-19: no Wi‑Fi set up, and it read as
                                            #   "this will work". Name the account by its email, then the state
bad:  ☁ lightplayer.app  river@example.com   Wi‑Fi not s…    # story, 10-19: at the grid's width the state was cut;
                                                            #   a state beside an email stays short
bad:  ☁ lightplayer.app  river@example.com   last seen 2…    # story, 10-20: the same cut; the surface says
                                                            #   "last seen" in words, the row just the age
```

### Access ○: what you can do

```text
○ Access    edit · USB                     # was: You can edit · USB
○ Access    edit · password                # was: You can edit · Workshop key
○ Access    edit · no password             # quiet: open is the default. was: yellow, "anyone can edit"
○ Access    play only        │ Unlock │    # was: You can play · anyone can play
○ Access    locked                         # primary: Unlock
○ Access    —        checking              # still: the link's own check. was: ◌ Access checking, bold, a sweep along its foot
◌ Access    —        unlocking             # a login you asked for: the row's work
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
▲ Firmware  none            │ Install │              # blank, or Studio erased it: lit, the primary's board pick. primary: Install firmware
▲ Firmware  unknown   no response                      # it never said hello and says nothing. primary: Install firmware
◌ Firmware  v2026.10.08-9   updating                   # carried: the surface has 42 %. was: updating · 42% and a rainbow fill
◌ Firmware  v2026.10.08-9   backing up │ Cancel │
◌ Firmware  —   installing                             # was: installing · 30%, in the name's place; Flashing firmware…
▲ Firmware  v2026.10.08-9   crashing     │ Reinstall │   # red; quiet while the surface says it
▲ Firmware  v2026.09.30-2   incomplete   │ Install │     # orange; quiet while the surface says it
▲ Firmware  v2026.10.08-9   confirmation required │ Review │   # was: Needs your answer
▲ Firmware  v2026.10.08-9   update interrupted │ Resume │      # was: Files waiting
▲ Firmware  v2026.10.16-1   data unreadable                    # shipped 10-15
▲ Firmware  v2026.10.16-1   update required                    # its files wait for a firmware that reads their store
▲ Firmware  v2026.10.08-9                      (dim)   # offline
```

The Firmware row's details say what is there in its own words (`Version
none`, `Version unknown`). On a board with no LightPlayer they carry no
verdict sentence: the surface says it, and the row (P2).

```text
bad:  Firmware   No response — try flashing firmware     # Yona's C6 10-19: a dashed label in a sentence's place,
                                                         #   a third time, on a board that was in fact blank
```

### Hardware ■: what the board is

```text
■ Hardware  XIAO ESP32-C6
■ Hardware  ESP32-C6       model not set         # was: ESP32-C6
■ Hardware  unknown                              # was: Board not known
◌ Hardware  —   identifying                      # was: identifying, in the name's place; chip unknown
■ Hardware  XIAO ESP32-S3 Plus                   # "simulated" is said once, on the USB row (P2)
```

Its details: the model, the chip, the board's id (its MAC, here once), a
stand-in's speed or tier and Reset; then, apart, the two verbs that lose
something. The name is renamed in the name bar, not here:

```text
│ Board      Model  XIAO ESP32-C6                 │
│            Chip   ESP32-C6                      │
│            Id     a0:f2:62:87:b4:8c             │
│ Actions    ↻ Reset                              │
│ Danger     ✕ Factory reset   ✕ Forget on this browser   ✕ Remove from your account │   Lasting: arm, then press again
```

Two forgets on a board the account's list holds (D10): **Forget on this
browser** (`devices/<board>/forget`) drops this browser's memory of it and
its port grant, and its words add that the board still shows from your
account on every computer; **Remove from your account**
(`devices/<board>/forget-from-account`) takes it off the account's list on
every computer: "Porch leaves your account's list on every computer. The
board keeps its keys, and it comes back the next time it reaches
lightplayer.app." Neither removes a key from the board: that is the Access
row's. Remove from your account is offered only while someone is signed in
and the list holds the board; pressed, the card leaves the list at once, and
if lightplayer.app refuses it the board comes back and its lightplayer.app
row's details say why.

```text
bad:  ✕ Forget          # on an account board, read as "gone everywhere": it is this browser's only
```

Factory reset erases the whole board (firmware, projects, its Wi‑Fi and
passwords), so it is the board's, not the firmware's: Yona, 2026-10-19,
"I expect to find factory reset in the hardware section". Its rules are the
offer's (`devices/<board>/erase`): USB only (refused with its reason over
another link), not while firmware is being written, and not where installing
is the repair (crashing, a version Studio can't get) or on a board with no
LightPlayer (nothing to erase). Its work runs in the Firmware row
(`◌ Firmware  erasing · 40%`, then `none`): work narrates where its subject
changes, so the erase and what it leaves read in one place. After it the
card is the blank board at once, heard or not: no Retry, no replug.

```text
bad:  ▲ Firmware  …  details: … Danger: Factory reset    # Yona's C6 10-19: looked for it under Hardware
bad:  This board stopped responding over USB.  │ Retry │ # Yona's C6 10-19, after the reset:
                                                        #   "it's basically a dead-end"
```

### Corner: the card's details

```text
╭───╮
│ ⋯ │                         every state: the details (every notice, the
╰───╯                         picture's facts, the terminal) one press away
                              connected: the panel's settings too —
                                ↺ Held controls   2 held        │ Reset │
                                ▣ Auto-save       Held values will come back after a restart.  (●)
                              # was: on the play page only
                              # was: (nothing) live · "editing" while the editor held it ·
                              #      "!" while the panel covered the rows. the surface says those now.
```

### Name bar and primary

```text
│ Porch lamp            │ ↔ Connect           │
│ Fabric Test           │ ☁ Connect           │   online at lightplayer.app, no link here
│ Porch lamp            │ ✓ Done              │   connected, reconnecting, in the editor
│ Porch lamp            │ ◌                   │   connecting, moving here from another tab
│ Porch lamp            │ ▲ Update firmware   │   old firmware; files that need newer firmware   # was: no button (the desk C6)
│ Porch lamp            │ ▲ Install firmware  │   no firmware, says nothing, incomplete, a version Studio can't get   # was: Install; says nothing was Retry (10-18)
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

**The name is renamed where it is.** A faint pencil after it, always
shown, brighter (and the name underlined) on hover. Pressed, a field takes
the name's place at the bar's height, its text selected; Enter or clicking
away keeps it, Esc puts it back; a blank or unchanged name changes nothing.
No Save button.

```text
│ XIAO ESP32-C6 ✎        │ + Add a project     │   the name, pressable
│ [ Holiday Eaves      ] │ + Add a project     │   renaming
```

The name keeps about twelve characters. On a card too narrow for that
beside the whole primary, the primary says its verb alone ("Update",
"Install", "Reinstall", "Resume", "Edit", "Add", "Connect"): the surface's
sentence already names the object. The button's text stays whole for a
screen reader.

## Decided

Yona, 2026-10-16 ("all leans"):

```text
O1  sending stays in place: ◌ Project  Playful Choker  sending
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
R7  offline, another way reaches it: the surface names that way; each row keeps its last value, dimmed
    (director's call; amended 10-19: the Link row is gone)
R8  a project's name in running text is quoted                     (director's call, pending Yona's veto)
R9  a row's value keeps its width; its state gives way first: cut, then gone   (director's review)
    (the kind column stays: one layout in one grid; a value still too long is cut at its end)
R10 one update, one number: the surface reads the working row's percent       (director's review)
R11 no dead ends: a fix that isn't offered says why and what to do instead    (director's review)
R12 a narrow name bar keeps the name's twelve characters; the primary says its verb alone   (director's review)
```

The card panel pass, 2026-10-19 (PR #1196, plan `lp2025/2026-10-19-0826-card-panel-pass`):

```text
C1  connected, the card holds every panel control; it grows once, on connect   (Yona)
C2  on a panel the clock is one Speed knob, 1× at the top; its card keeps the tape   (Yona)
C3  on a panel brightness is a knob, still first; its fixture's card keeps the fader   (Yona)
C4  the panel's reset and auto-save live in the ⋯                  (director's call, pending Yona's veto)
C5  Edit, or its lock, stays at the bottom, at the panel's foot    (director's call, pending Yona's veto)
```

Yona's C6 install walk, 2026-10-19:

```text
W1  Factory reset is the Hardware row's: it erases the board, not only its firmware (Yona)
W2  a board Studio erased is the blank board at once, heard or not: Install firmware, no Retry (Yona)
W3  the Firmware details say what is there in the row's words; the surface says the sentence (Yona)
```

The setup rows, 2026-10-19 (plan `lp2025/2026-10-19-1350-board-setup-rows`, spike `spikes/install-network-setup`):

```text
S1  the rows are a new board's setup list: eight rows, the links in the middle   (Yona)
S2  a row with something left to do is lit: the whole row, flat, no gradient, no dot   (Yona: "the lit row alone")
S3  the state says the fact, the button the fix: "not connected" on USB, Wi‑Fi and lightplayer.app   (Yona)
S4  hushing a row is disabling it from its details; the row then says "disabled"   (Yona)
S4a …with one switch line atop the details, the same for Wi‑Fi, Bluetooth and lightplayer.app, as macOS draws it   (Yona, the gate)
S5  the board is renamed where its name is, a faint pencil always shown   (Yona)
S6  the primary is Install firmware, Add a project, Connect; Wi‑Fi and lightplayer.app ask through their rows   (Yona)
S7  the struck cloud mark is gone: lightplayer.app's row says it   (Yona)
S8  stand-ins keep the eight rows, "—" on the radio rows   (Yona)
```

The update feel pass, 2026-10-24 (FS3 over Bluefy; PR #1298):

```text
U1  one work, one bar, one number: the surface's; the row doing it is carried   (Yona)
U2  a carried row keeps its spinner, not its fill: "some indicator on the row that something
    is happening is nice … just the rainbow icon, but not the bar"              (Yona)
U3  a check nobody asked for is still: Access  —  checking                      (Yona)
U4  the name is the value in every state, never bold for work; the work's words are the state   (Yona)
U5  an update's number only moves forward                                         (Yona)
```
