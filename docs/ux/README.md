# Studio UX

How Studio shows things. Read this before building or reviewing any UI.

Each principle is one rule, a do and a bad. The bad lines are real: each one
shipped, or nearly did, and the comment says where it was caught. Start from
the index, pick the principles your change touches, and read only those.

```text
docs/ux/README.md             the principles (this file)
docs/ux/anatomy/<thing>.md    one component, drawn whole, then part by part
docs/ux/studies/<topic>/      deep dives: rendered good/bad pages
```

Notation:

```text
│ … │          a surface: a card, a bar, a row
│ Send │       a button, flush in its bar
◆ ↔ ○ ▲ ■      icons: project, link, access, firmware, hardware
◌              working (spinner)
━━━━────       progress
(dim)          drawn stale
← red          the tone the line takes (ui.md's status families)
# card 10-14   where the bad was caught: a feel gate, a spike round, a review
```

## Index

```text
P1   teach what it is              any label, row, button
P2   say it once                   a fact or problem shown in two places
P3   stay in place                 anything with states
P4   quiet when good, loud when bad  status, indicators, numbers
P5   details on demand             anything longer than a label
P6   label voice                   any words
P7   no dead ends                  disabled controls, errors, empty states
P8   local consistency             rows, lists, cards, pages
P9   useful structure              lines, boxes, sections, icons
P10  coherence over css ease       borders, dividers, progress, popups
P11  their words, not ours         any words
P12  the button explains           buttons
P13  old looks old                 offline, cached, last-known
P14  never colour alone            tones, marks
P15  numbers read naturally        numbers, times, sizes
P16  one treatment per meaning     icons, colours, words that mean something
P17  one way per kind of data      times, sizes, versions, names, status
P18  the main surface says what matters  cards, pictures, the top of a page
P19  the primary is the likely next step  the button beside a name
P20  voice by role                 any words: labels, sentences, buttons
```

P1–P10, P16–P20: Yona (P18–P20 set 10-17). P11–P15: the director's, accepted 10-16.

---

## P1. teach what it is

Say what a thing is. A few values explain themselves (`v1.2.3`, `USB`).
Most don't: give them their kind, as an icon and a word.

```text
do:   ◆ Project   studio
do:   ▶ Simulated · off
bad:  ◆ studio                         # shipped: a bare name, of what?
bad:  Playing studio                   # spike r1: "confusing, though I know what you're getting at"
bad:  In this tab · off                # card 10-14: "does that mean it's emulated?"
bad:  Nothing on it yet                # card 10-14: it has firmware; the project is what's missing
```

anatomy: [board card](anatomy/board-card.md) (round 2, shipped) ·
app: `lpa-studio-core/…/board_card/project_bar.rs`

## P2. say it once

One fact, one place. One problem, one place: its row turns red and says
what's wrong, and nothing else on the screen repeats it.

```text
do:   ▲ Firmware  v2026.10.08-9   crashing │ Reinstall │    ← red
      (the corner and the name bar stay quiet)

bad:  │ XIAO ESP32-C6       │ Updating · 42%  │   # shipped: the button says it
      │ ◌ Updating · 42%  ━━━━━━────────      │   # …and the row says it again
bad:  XIAO ESP32-S3 Plus (sim) · Simulated · off · Simulated XIAO …   # shipped: 3× on one card
bad:  You can edit · anyone can edit                                  # shipped: twice in one row
```

One exception: the primary button may repeat a row's own button (P19).

app: `lpa-studio-core/…/board_card/status_corner.rs`, `primary_action.rs`

## P3. stay in place

Everything keeps its place in every state: kind, name, state, action.
A state never moves in front of the name, and the name never moves out.
Pixels too: a live number doesn't push the layout around.

```text
do:   ◆ Project   studio           out of date │ Send latest │
do:   ▲ Firmware  v2026.07.02 (pre-OTA)   │ Update to v2026.10.16-1 │

bad:  ◆ Out of date                studio     # shipped: the name moved to the right
bad:  Old LightPlayer firmware                # spike r1: a word in front of the firmware
bad:  the card leaves Online boards for Offline boards and comes back, every few seconds,
      its open details closed                 # C6 walk 10-19: an erased board re-enumerating,
                                              #   "jarring and resets the ui"
```

A board's events never reset what someone is doing on its card: open
details stay open, an armed button stays armed, and the card keeps its
place.

app: `lpa-studio-core/…/board_card/ui_stack_bar.rs` (icon · summary · aside · tone · action)

## P4. quiet when good, loud when bad

Normal says nothing. Trouble can't be missed. Don't label the usual
state, or anything nobody can act on.

```text
do:   (the corner is empty while the picture is live)
do:   ▲ Firmware  v2026.10.16-1
do:   ▲ Firmware  v2026.10.08-9   crashing │ Reinstall │    ← red

bad:  ● 899 fps                       # shipped: the corner, for us
bad:  Live                            # spike r1: "means nothing to me… that should be the default"
bad:  Firmware up to date             # spike r1: the version already says it
```

## P5. details on demand

The surface answers three questions: what is it, is it ok, what's next.
Everything else is one step in, in the row's details.

```text
do:   ◆ Project   studio   error                          ← red
      └ details:  orbit.shader:12  'tme' is not declared

bad:  ◆ studio has a problem · a shader didn't compile     # spike r1
bad:  Needs 2026.09.30, which Studio can't get             # shipped: too long for a row
bad:  This board's file store has a newer or damaged header; its files
      are kept — read them with `lp-cli hardware tree extract`.   # main 10-15: a paragraph on a card
```

app: `lpa-studio-web/…/board_card/bar_details.rs` (the detail card)

## P6. label voice

Labels, not sentences, in the words other software already uses. No
pronouns, no chat, no explaining. A sentence goes in the details, if anywhere.

```text
do:   Confirmation required · Data lost · Not connected · Disabled
      Inactive · Awaiting input · Update available · Interrupted

bad:  Update needs your OK            # spike r1: "so ai-slop"
bad:  Its files are gone              # card 10-14
bad:  Needs your answer               # shipped: answer to what?
bad:  The board isn't online.         # shipped: say "Offline"
bad:  Format 3 — too old for this Studio    # sweep 10-17: "Format too old (3)"
bad:  Changes — 3 unsaved                   # sweep 10-17: "Changes: 3 unsaved"
bad:  Firmware   No response — try flashing firmware   # C6 walk 10-19: a sentence with a dash where
                                                       #   "Version  none" goes, on a blank board
```

P6 governs labels. P20 says where a sentence is allowed, and how it reads.

## P7. no dead ends

Never leave someone with nothing to do. A control that can't be pressed is
replaced by the step that works, or says why. A problem offers its fix and
a way out.

```text
do:   │ XIAO ESP32-C6        │ + Add a project │      # the next step that works
do:   ◆ Project   none  data lost  │ Restore │ Reset │      # the fix, and a way out (O3)

bad:  │ XIAO ESP32-C6        │ Connect         │  (grey)      # card 10-14: "confusing"
bad:  Its files need restoring                        # card 10-14: "what if I just want to reset it?"
bad:  Not known yet                                   # card 10-14: what isn't known?

do:   │ This board doesn't have LightPlayer firmware.    │   # Studio erased it, so it knows: no
      │ Install the firmware to use it.                  │   #   waiting for a board that can't answer
      │ XIAO ESP32-C6                 │ Install firmware │
bad:  │ This board stopped responding over USB.          │   # C6 walk 10-19, right after a Factory reset:
      │ XIAO ESP32-C6                            │ Retry │   #   Retry timed out, only a replug got out,
                                                           #   "it's basically a dead-end"
```

app: `lpa-studio-core/…/board_card/primary_action.rs`

## P8. local consistency

Siblings share one shape: rows in a card, cards on a page, pages in the
app. Find the pattern, name it, reuse it. A one-off needs a reason.

```text
do:   ◆ Project   studio
      ⏚ USB       connected
      ≋ Wi‑Fi     Cedar Lane
      ○ Access    edit · USB
      ▲ Firmware  v2026.10.16-1
      ■ Hardware  XIAO ESP32-C6

bad:  studio                     # shipped: a name alone
      USB · live                 #          a link and a state
      You can edit · USB         #          a sentence
      2026.10.08                 #          a value alone
      XIAO ESP32-C6              #          five rows, four grammars
```

The card's bars share one shape in code (icon · summary · aside · tone ·
action). Their words had no shared grammar until the anatomy gave them one:
[board card](anatomy/board-card.md).

The same holds for one control across siblings. Three radio rows that can
each be turned off turn off the same way, in the same place:

```text
do:   ≋ Wi‑Fi              (●━━)    # the first line of each row's details: icon, name,
      ᛒ Bluetooth          (●━━)    #   the board's own switch at the line's end,
      ☁ lightplayer.app    (●━━)    #   as macOS draws the top of its Wi‑Fi and Bluetooth menus

bad:  Disable Wi‑Fi under the network list · Disable lightplayer.app beside Sign in ·
      a red Disable Bluetooth in an Actions list
                                    # Yona 10-19: "rendered differently in each of the three sections"
```

## P9. useful structure

Every line, box, icon and word is there to say something. If you can't
answer "why is that there?", remove it. Never put a box inside a box.

```text
do:   ┌──────────────────────┐        bad:  ┌──────────────────────┐
      │ Porch lamp           │              │ ┌ Board ───────────┐ │
      ├──────────────────────┤              │ │ ┌ Info ────────┐ │ │
      │ ◆ Project   studio   │              │ │ │ ◆ studio     │ │ │
      │ ⏚ USB       connected│              │ │ └──────────────┘ │ │
      └──────────────────────┘              │ └──────────────────┘ │
                                            └──────────────────────┘
```

`›` at a row's end is structure with a reason: the row opens its details.

## P10. coherence over css ease

Draw what the design means, not what CSS made easy. A line that divides a
box crosses the whole box. Things that belong together share one outline:
a button and the popup it opens are one shape (the detail card).

```text
do:   │ Porch lamp            │ Connect │     # the action is a section of the bar;
      └───────────────────────┴─────────┘     # its line runs edge to edge
bad:  │ Porch lamp        ( Connect )   │   # a chip floating in the bar

do:   │ ◌ Updating · 42%                   │
      │━━━━━━━━━━━━━━──────────────────────│  # the progress stays in its row
bad:  │ ◌ Updating · 42%                   │
      ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  # card 10-14: the bar escaped its row
```

app: `lpa-studio-web/src/base/detail_popover.rs`

---

## P11. their words, not ours

Name things the way the person sees them, never the way the code does.
Internal words stay in debug surfaces.

```text
do:   ▲ Firmware  v2026.10.16-1

bad:  fw-esp32c6 c688c990b           # shipped: a build label
bad:  Pre-hello firmware             # shipped: our protocol history
bad:  Updating · 1 of 2 · 42%        # shipped: core, then engine; one number will do
bad:  Files waiting                  # shipped: the mechanism, not the state
bad:  Live preview on the GPU tier                       # sweep 10-17: "on the GPU"
bad:  Install a project from a pasted JSON envelope.     # sweep 10-17: "Add a project you copied as JSON."
bad:  Already live in this session — Save banks v13 to your library.   # sweep 10-17
bad:  Revert on /orbit could not reach the server overlay: …           # sweep 10-17: "Couldn't revert /orbit: …"
bad:  no writer — authored default (R6)                  # sweep 10-17: a plan id on screen
```

## P12. the button explains

A button says its verb, and its object when that isn't obvious. Let it
carry the meaning so the line doesn't have to.

```text
do:   ▲ Firmware  v2026.10.08-9   │ Update to v2026.10.16-1 │   # newer, without saying "old"

bad:  │ Install │                         # shipped: install what?
bad:  Old firmware · install the latest   # spike r1: the button already says it
```

## P13. old looks old

A stale fact keeps its place, drawn stale. Show the last thing known, not
"unknown". Say how old once, in one place.

```text
do:   ↔ Offline · 2 h
      ◆ Project   studio          (dim)
      ○ Access    edit            (dim)

bad:  Project not known                 # shipped: it was known two hours ago
bad:  Was playing studio · last seen    # spike r1: "last seen" on every row
```

## P14. never colour alone

Colour says how bad. A word or a shape says what. Pair every tone with one.

```text
do:   ▲ Firmware  v2026.10.08-9   crashing │ Reinstall │   ← red: the word says what

bad:  ↔ USB · connected  ☁ (lit)   vs   ☁ (dim)   # shipped: only colour tells them apart
```

## P15. numbers read naturally

Pick the unit that makes the number easy to read, and use one format
everywhere.

```text
do:   2 h · 3 d · 2 weeks · 42% · 3.2 MB

bad:  2 h ago  ·  Offline · 2 h      # shipped: one time, two formats, one card
```

P17 gives each kind of number its one component.

## P16. one treatment per meaning

A meaning gets one icon, one colour, one word, everywhere in the app.
Reuse what exists. Never borrow a colour, icon or word that already means
something else.

```text
do:   ▲ Firmware  v2026.10.08-9   │ Update to v2026.10.16-1 │   # no tone: the button carries it
do:   ⏚ USB       not responding  │ Retry │   ← orange           # orange: a device needs you

bad:  ○ You can edit · anyone can edit         ← yellow   # shipped: yellow means an unsaved edit
bad:  ▲ 2026.10.08                │ Update │  ← blue     # shipped: blue means live
```

app: `lpa-studio-core/src/core/status.rs` (`UiStatusKind`) ·
`lpa-studio-web/src/base/icon.rs` (`StudioIconName`) ·
`docs/style/ui.md` (the status families) · `docs/style/language.md` (the verbs)

## P17. one way per kind of data

Each kind of data has one type and one component that draws it, used
everywhere and shown in one story: times, sizes, percentages, versions,
project names, board names, status.

```text
do:   age_words(5_400)      → "2 h ago"       # one rule for every surface that names an age
      duration_words(5_400) → "2 h"

bad:  format_bytes · human_bytes · size · byte_size     # shipped: four size formatters;
                                                        #   1536 B reads "1 KB" in one, "1.5 KB" in three
bad:  12 s ago · last frame · 12 s ago · last heard 12 s ago · quiet — last heard 12 s ago
                                                        # shipped: one age, four phrasings
```

app: `lpa-studio-core/src/core/data_kinds/` (ages, byte sizes, percents,
dates, firmware versions) · story: `core/data-kinds/every-kind` (every kind,
every case)

## P18. the main surface says what matters

"The primary surface should always give the most important information."
The top of a card, where the picture is, always shows the most important
thing about the board. It is never black. A problem that stops the board
being used is said there, not in a quiet aside further down.

```text
do:   ┌──────────────────────────────────────────────┐
      │ This board is running old firmware.          │   ← the picture's place
      │ Update the firmware to connect and edit.     │
      ├────────────────────────────┬─────────────────┤
      │ XIAO ESP32-C6              │ Update firmware │
      └────────────────────────────┴─────────────────┘

bad:  ┌──────────────────────────────────────────────┐   # card 10-17: a black picture,
      │ (black)                                      │   #   no button up top, and the
      ├──────────────────────────────────────────────┤   #   reason a quiet aside in a row
      │ XIAO ESP32-C6                                │
      │ ▲ Firmware  v2026.07.02   too old            │
      └──────────────────────────────────────────────┘
```

## P19. the primary is the likely next step

"The primary action slot should hold the most likely action a user wants to
take." The button beside the name holds it. It may repeat a row's own
button: the one exception to P2.

```text
do:   │ XIAO ESP32-C6                  │ Update firmware │   # the likely next step, up top,
      │ ▲ Firmware  v2026.07.02                 │ Update │   #   and still in its own row

bad:  │ XIAO ESP32-C6                                    │   # card 10-17: no button up top;
      │ ▲ Firmware  v2026.07.02                 │ Update │   #   the fix a red Update in a row

do:   │ This board doesn't respond.                      │   # a new board that never said hello
      │ Install the firmware to use it.                  │
      │ New device                    │ Install firmware │

bad:  │ This board stopped responding over USB.          │   # lightplayer.app 10-18, a first-time user:
      │ New device                               │ Retry │   #   "'board not responding' is sort of a 'duh,
                                                           #   I'm trying to program it'"
```

app: `lpa-studio-core/…/board_card/primary_action.rs`

## P20. voice by role

Every line has a role, and the role sets its voice.

- **Labels** (row states, chips, headings, statuses): P6. The words other
  software uses: "Connection failed", "Not connected", "Update available".
  No pronouns, no chat, no dashes or semicolons.
- **Sentences**, only where a surface explains (an empty state, a lede under
  a heading, a dialog's body, the card's top): one or two short, complete
  sentences. Say what is going on, then what it means for you. Keep the
  articles and verbs.
- **Buttons**: a verb and its object (P12).

When a button sits under the sentences, the second sentence is the button
said aloud: its verb and object, then what it gets you.

Tense: plain speech uses "is …ing" for a state and "will" for a
consequence. The simple present narrating one ("keeps playing", "takes it
over", "the project stays") is an AI tell. "Keep" + -ing is only for
repetition ("keeps crashing").

The AI tells, each one a bad below: headline-ese, our mechanism instead of
their situation, unasked reassurance, lines that sound meaningful and say
nothing, marketing clichés.

```text
do:   This board is running old firmware. Update the firmware to connect and edit.
do:   Open one to try it, then save it to keep your own copy.
do:   Something else is driving this channel. Turning the control will take it over.

bad:  Firmware too old for this Studio / its project keeps playing   # director, 10-17
bad:  This board is running old firmware. An update is required to connect and edit.
                                          # spike r1: says the button indirectly
bad:  Connecting here takes it over       # director, 10-17: present for a consequence
bad:  Real pieces, wired the way they were built. Open one here, then make it yours.
                                          # home, 10-17
bad:  Play freely — nothing saves until you keep a copy.          # sweep 10-17: chat, then a dash
bad:  Saved to the running project, but not yet to your library — will retry on the next save
                                          # sweep 10-17: "will retry" with no subject
bad:  Could not add that address — nobody was invited.            # sweep 10-17: unasked reassurance
bad:  Fork — make it yours                # sweep 10-17: a dash in a button, and says nothing
bad:  Something else drives this channel; turning the control takes it over.
                                          # sweep 10-17: present for a consequence
```

---

## Keeping it alive

```text
before    UI work starts here: name the principles it touches, read the anatomy
plan      a UI plan draws its anatomy first, in ASCII, reviewed before code
review    a feel-gate note becomes a bad → do pair here, in the PR that fixes it
new       no principle fits? add one, marked proposed; Yona approves it
study     a converged spike becomes a study, linked from its principles
data      a new kind of data gets its type, its component and its story (P17)
```
