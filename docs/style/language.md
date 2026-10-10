# Lightplayer Language

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
  link's icon says how (USB, Bluetooth, Wi‑Fi, the cloud). On a board
  another tab holds, Connect takes it over.
- **Done** — close it; the card shows its facts again.
- **Edit** — open the board's project in the editor. It's the project
  bar's action, for people who can edit. Until Connect lands, Edit is the
  card's primary on a ready board that runs a project.
- **Unlock** — enter a password for a locked board.
- **Install** — put Lightplayer on a blank board.
- **Update** — the firmware bar's action when a newer version exists
  ("when it's back" if the board is offline).

What each bar says:

| Bar | Says | Examples |
|---|---|---|
| Project | what the board plays, and how many boards share it | `Holiday Eaves · 3 boards`, `Nothing on it yet`, `Out of date` |
| Connection | the link and its state | `USB · live`, `Bluetooth · connected`, `Wi‑Fi via lightplayer.app · live`, `also cloud`, `direct only`, `Offline · 2 weeks`, `Sean is editing` |
| Access | what you can do, and with which key | `You can edit · USB`, `You can edit · your account key`, `You can play`, `Locked` |
| Firmware | the version alone, whatever it is | `2026.10.08-9`, `dev 5eb70a7` |
| Hardware | the board model | `XIAO ESP32-C6`, `Emulated XIAO ESP32-C6` with `in this tab` beside it |

A board reached through lightplayer.app names that link in full, **Wi‑Fi
via lightplayer.app**, with the cloud icon. **also cloud** and **direct
only** appear only once the board has said whether its Cloud relay is on;
before that the connection bar says nothing about the cloud. Where Studio
cannot know a fact yet (a locked board's project, an offline board's
access), the bar says **Not known yet**.

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
error tint when that tab has the editor open) and it is disabled, with the
reason ("Busy in the other tab: Updating · 42%"), while that tab is
flashing, updating or pushing. A tab that doesn't answer in 5 s is "That
tab didn't answer", with Retry.
