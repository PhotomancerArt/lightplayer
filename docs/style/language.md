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
  firmware on it, and nothing to show yet): the project pick. The project
  bar, which says **No project**, does not repeat it.
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
- **Install** — put Lightplayer on a blank board.
- **Start fresh** — on a board whose files are gone: let them go and leave
  the board empty for a new project. Nothing on the board changes.
- **Update** — the firmware bar's action when a newer version exists
  ("when it's back" if the board is offline).

**A primary is never a greyed verb.** When there is nothing to press, the
primary says why in its own word: **Connecting…**, **Updating · 42%**,
**Offline · 2 weeks**, **Identifying…**, **Busy in the other tab**,
**Asking the other tab…**. Never a disabled "Connect" with the reason in a
tooltip. And a reason shown under a disabled button is the reason alone
("No project"), never "this cannot be done right now: …".

While Connect works, the connection bar carries it: **Connecting…** (the
primary reads the same), **Reconnecting…** when the connected
board's link dropped and Studio is holding on for it, and **Couldn't
connect**, striped, with Retry, when it failed (the reason is in the
details; a board that never answered is "The board didn't answer in
time"). "Couldn't connect" goes once the board is reached another way. A
chooser closed with nothing picked is not a failure: the card is back at
Connect. Connect on a second board hands over: the first card shows its
facts at once.

What each bar says:

| Bar | Says | Examples |
|---|---|---|
| Project | what the board plays, and how many boards share it | `Holiday Eaves · 3 boards`, `No project`, `Project not known`, `Out of date` |
| Connection | the link and its state | `USB · live`, `Bluetooth · connected`, `Wi‑Fi via lightplayer.app · live`, `Offline · 2 weeks`, `Simulated · running`, `Emulated · off`, `Sean is editing` |
| Access | what you can do, and with which key | `You can edit · USB`, `You can edit · in this tab`, `You can edit · your account key`, `You can play`, `Locked`, `Access not known · when it's back` |
| Firmware | the version alone, whatever it is | `2026.10.08-9`, `dev 5eb70a7`, `Firmware not known`, `Its files are gone` |
| Hardware | the board model | `XIAO ESP32-C6`, `Emulated XIAO ESP32-C6` with `in this tab` beside it, `Board not known` |

A board reached through lightplayer.app names that link in full, **Wi‑Fi
via lightplayer.app**, with the cloud icon. A board reached directly that
is also reachable through lightplayer.app shows the cloud icon alone at the
connection bar's end, no words: lit while lightplayer.app can reach it, its
label and hover "Also reachable through lightplayer.app". A board reached
only directly shows nothing. Neither appears until the board has said
whether its Cloud relay is on.

**Every bar names its subject.** "No project" where the board said it runs
none (there is firmware on it: never "Nothing on it yet"); **Project not
known**, **Access not known**, **Firmware not known**, **Board not known**
only where Studio has genuinely heard nothing. Never a bare "Not known yet".

**A stand-in says what it is.** A simulated board's connection bar reads
**Simulated · running** or **Simulated · off**; a board emulated in this
tab, **Emulated · running** or **Emulated · off**. Its hardware bar says the
same word before the board (**Simulated XIAO ESP32-C6**, "in this tab"
beside it), and its access is known whether it runs or not: **You can edit
· in this tab**.

**A board's files are gone** only when Studio has evidence it had some (a
backup in this browser, or this browser saw it run a project). A freshly
installed board never says so. When it is real, the firmware bar says **Its
files are gone**, and the notice says what to do: Restore files, Restore
from a backup file…, or Start fresh.

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
updating or pushing there is nothing to press, and the primary's word is
**Busy in the other tab** (the work on its hover: "Busy in the other tab:
Updating · 42%"). A tab that doesn't answer in 5 s is "That tab didn't
answer", with Retry.
