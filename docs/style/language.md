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

The board card's verbs:

- **Connect** — open the board here and show its panel on the card. The
  link's icon says how (USB, Bluetooth, Wi‑Fi, the cloud).
- **Done** — close it; the card shows its facts again.
- **Edit** — open the board's project in the editor. It's the project
  bar's action, for people who can edit.
- **Unlock** — enter a password for a locked board.
- **Install** — put Lightplayer on a blank board.
- **Update** — the firmware bar's action when a newer version exists
  ("when it's back" if the board is offline).

What each bar says:

| Bar | Says | Examples |
|---|---|---|
| Project | what the board plays, and how many boards share it | `Holiday Eaves · 3 boards`, `Nothing loaded` |
| Connection | the link and its state | `USB · live`, `Bluetooth · connected`, `also cloud`, `direct only`, `Offline · 2 weeks`, `Sean is editing` |
| Access | what you can do, and with which key | `You can edit · your account key`, `You can play`, `Locked` |
| Firmware | the version alone; its date is in it | `2026.10.08-9` |
| Hardware | the board model | `XIAO ESP32-C6`, `Emulated C6 · in this tab` |

**Cloud connected** names a board that talks to lightplayer.app through
the relay; it's a property of the board, not of the link in use. A board
that doesn't is **direct**.

When another person or tab holds a board, say who: **"Sean is editing"**,
**"Open in another tab"**, or **"Someone else connected"** when Studio
can't tell who. Offer to take over when Connect is pressed, not on the
card.
