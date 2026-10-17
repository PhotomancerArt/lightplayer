# Lightplayer Language

The principles behind these words and looks, with examples, are in
`docs/ux/README.md`.

## Product Names

Use **Lightplayer** as the umbrella product and brand name. Treat **Light
Player** as descriptive prose only when the sentence is intentionally about
playing light, not the product name.

Use surface names when the distinction matters:

- **Lightplayer Studio** for the frontend design and control app.
- **Lightplayer Firmware** for the ESP32 device firmware.
- **Lightplayer Engine** for the runtime, compiler, and node graph layer.
- **Lightplayer Compiler** for the GLSL to LPIR to machine-code pipeline.
- **Lightplayer Native** only when the custom RV32 backend needs a proper name.

In general prose, prefer **Lightplayer** when the surface does not matter. Do
not make **Studio** stand alone as the public product name; internally, `studio`
is fine as a short implementation name.

## Studio UI Language

Studio UI should present the user's working model first and the system model
second.

Use the fewest words that preserve meaning. Main UI should name the thing,
show the value, and stop. Longer explanations belong in details, source, or
debug surfaces.

## Names Before Types

Prefer names, labels, and authored concepts in primary UI.

- Use `output visual`, not `Visual product`.
- Use `blast shader`, not a large `Shader` badge competing with `blast`.
- Use `time`, `brightness`, and `center`, not technical slot categories as the
  first thing the user reads.

Types are supporting language. They can appear inline, in lowercase, near the
name when they help orientation.

## Keep Technical Detail In Debug Surfaces

Revision numbers, wire slot roles, internal product refs, binding mechanics,
and implementation vocabulary belong in debug/source panes unless the user is
explicitly editing that concept.

Main-level node UI should avoid showing labels such as `rev 42`, `consumed`,
`uniform`, `binding`, or `ProductRef`. Those facts are still important, but
they should live in a technical tree or debug pane where their density is
useful.

## Product Presentation

For visual and control products, the preview is the main event. The main view
should give the preview most of the space and use a restrained single-line
caption:

```text
output visual (128 x 72)
```

Use formal type names such as `VisualProduct` or `ControlProduct` only in
debug/source surfaces.

## Slots

Slot rows should read like fields in a familiar editor:

```text
[source] time        ../playlist#entry_time
[source] brightness  0.72
```

The source icon can indicate direct value, binding, or child pointer. Detailed
binding source metadata and revisions should be discoverable, but not always
visible in the main node surface.

## Boards and the home page

**Board** is the UI noun for a thing with lights that Lightplayer runs:
"Online boards", "Connect a board". Use "device" in code and in debug
surfaces, not on the home page or the card. (A desktop server is not a
board; revisit when one exists.)

The home page's sections, in order: **Online boards**, **Connect a
board**, **Offline boards**, **Other projects**, **Your patterns**, then
the examples. Its tabs are **All**, **Boards**, **Projects** and
**Patterns**.

The rest of the page's words:

- The catalog's two sections are **Example projects** and **Example
  patterns**. The archive drawer is **Archived projects**. The Projects
  tab's own section, every project in the library, is **Projects**.
- Under the boards sits a closed fold, **Unlocking your boards**.
- Connect a board's squares read **USB**, **Bluetooth** and **Network**.
  The quiet verb under them is **start a board here**. A first visit adds
  one hint line: "No board? Try an example ↓".
- Someone with something to keep and no account sees one line: "Sign in to
  unlock your boards from any browser." A first visit doesn't.

The board card's verbs:

- **Connect** — open the board here and show its panel on the card. The
  link's icon says how (USB, Bluetooth, Wi‑Fi, the cloud). It means that
  one thing on every board you can reach: on a closed port or an offline
  board it reaches the board first, then shows the panel, in one press. On
  a board another tab holds, Connect takes it over.
- **Add a project** — the primary of a board running no project (there is
  firmware on it, and nothing to show yet): the project pick. The Project
  row, which says **none**, does not repeat it.
- **Done** — close it; the card shows its facts again. It is the
  connected card's primary, and the editor's docked card's.
- **Edit** — open the board's project in the editor. It's the project
  bar's action, for people who can edit; on a board that isn't connected
  it connects first. While the board is connected, Edit sits at the end of
  the All controls row. On a board you can only play, Edit wears a lock
  and asks for the edit password (`unlock`).
- **All controls** — the connected card's last row, a link to the board's
  play page: **All controls · 2 more** when the page has controls the card
  leaves out.
- **Unlock** — enter a password for a locked board.
- **Install firmware** — put Lightplayer on a blank board (the name bar's
  primary). On the Firmware row, beside a version, the verb is **Install**:
  the row already says it is firmware.
- **Restore** and **Reset** — on a board whose data was lost: put the
  backup back, or let it go (once "Start fresh") and leave the board empty
  for a new project. Nothing on the board changes for Reset.
- **Update** — the Firmware row's action when a newer version exists. The
  version it brings is the details' "Newest": "Update to v2026.10.16-1"
  does not fit beside the board's own version at the card's width.
- **Resume** — finish an update that stopped half-way.
- **Review** — open a confirmation the details ask for.

**No dead ends, and no repeats.** When nothing can be pressed there is no
primary at all — never a greyed verb, and never a row's words said again on
the name bar: the busy, away or waiting row says why, once. While the board
is being reached or identified the primary is the spinner alone. And a
reason shown under a disabled button is the reason alone ("No project"),
never "this cannot be done right now: …".

While Connect works, the Link row carries it, naming the link: **USB ·
connecting**, **USB · reconnecting** when the connected board's link
dropped and Studio is holding on for it, and **USB · connection failed**,
red and striped, with Retry, when it failed (the reason is in the details;
a board that never answered is "The board didn't answer in time"). The
failure goes once the board is reached another way. A chooser closed with
nothing picked is not a failure: the card is back at Connect. Connect on a
second board hands over: the first card shows its facts at once.

Every row reads **icon · kind · name · state · action**
(`docs/ux/anatomy/board-card.md`): the kind is a muted word ("Project",
"Access", "Firmware", "Hardware"), the name is the thing itself and stays
in place in every state, and the state is said only when it is not normal.
The Link row has no kind word: the link's name is its kind.

| Row | Name · state | Examples |
|---|---|---|
| Project | what the board plays, and how it stands | `studio`, `studio · 2 boards`, `none`, `studio · sending · 40%`, `studio · out of date` (Send latest), `studio · error`, `hidden`, `none · data lost` (Restore, Reset) |
| Link | the link and how it stands | `USB · connected`, `Bluetooth · connected`, `lightplayer.app · connected`, `USB · connecting`, `USB · not responding`, `USB · connection failed`, `Offline · 2 weeks`, `Simulated · running`, `Emulated · off`, `Open in another tab` |
| Access | the tier, and how it is held | `edit · USB`, `edit · password`, `edit · no password`, `play only` (Unlock), `locked`, `checking`, `USB only · no password`, `edit` (a stand-in) |
| Firmware | the version, Studio's one way | `v2026.10.16-1`, `dev c688c99`, `v2026.10.02-1 (pre-OTA)`, `unknown (pre-2026.07)`, `none`, `v2026.10.08-9 · updating · 42%`, `· crashing` (Reinstall), `· incomplete` (Install), `· confirmation required` (Review), `· update interrupted` (Resume), `· data unreadable` |
| Hardware | the board model | `XIAO ESP32-C6`, `ESP32-C6 · model not set`, `unknown`, `identifying` |

A version is always Studio's one firmware version: a release as its tag
(`v2026.10.16-1`), a local build `dev c688c99`, never the build's target
prefix (`fw-esp32c6 …`: the details' "Build"). An update reads one number
for the whole of it, never "1 of 2".

A board reached through lightplayer.app names that link, **lightplayer.app**,
with the cloud icon. A board reached directly that is also reachable
through lightplayer.app shows the cloud icon alone at the Link row's end,
no words: filled while lightplayer.app can reach it, struck through while
Cloud relay is on and it can't, its label and hover "Also reachable through
lightplayer.app". A board reached only directly shows nothing. Neither
appears until the board has said whether its Cloud relay is on.

**Every row names its subject**, with its kind word: `Project none` where
the board said it runs nothing (there is firmware on it: never "Nothing on
it yet"), `unknown` only where Studio has genuinely heard nothing. Never a
bare "Not known yet". **Old looks old:** an offline board's rows keep the
last thing known, dimmed, and only the Link row says since when.

**A stand-in says what it is, once.** A simulated board's Link row reads
**Simulated · running** or **Simulated · off**; a board emulated in this
tab, **Emulated · running** or **Emulated · off**. Its Hardware row names
the model alone, and its access is known whether it runs or not:
**edit**.

**A board's data is lost** only when Studio has evidence it had some (a
backup in this browser, or this browser saw it run a project). A freshly
installed board never says so. When it is real, the Project row says
**none · data lost**, with Restore and Reset beside it, and the details
hold Restore from a backup file… and Download backup.

A new board is named after its model ("XIAO ESP32-C6"), with no date; a
second of the same model gets a number ("XIAO ESP32-C6 2").

**Cloud connected** names a board that talks to lightplayer.app through
the relay; it's a property of the board, not of the link in use. A board
that doesn't is **direct**.

When another person or tab holds a board, say who:

- **Open in another tab** — another tab of this browser has the board.
  Its aside says what that tab is doing: "editor open", or the work it is
  busy with.
- **Taken by another tab** — this tab had the board and let go because
  another tab asked.
- **Someone else connected** — a holder Studio can't name, on the board's
  one network connection (a person, or another browser). The card offers
  Retry and nothing else.
- **"Sean is editing"** — a person, once Studio knows who (step 3).

**Connect is the take-over** on a board another tab holds: there is no
separate button on the card. Its tint says what it closes over there (the
error tint when that tab has the editor open). While that tab is flashing,
updating or pushing there is nothing to press: no primary at all, and the
Link row's state says what that tab is busy with. While the take-over runs
the primary is the spinner alone. A tab that doesn't answer in 5 s is
"That tab didn't answer", with Retry.
