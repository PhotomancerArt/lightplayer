# ADR: Patterns Declare Their Inputs

- **Status:** Accepted
- **Date:** 2026-10-20
- **Deciders:** Photomancer (Yona's rulings are the playlists-on-the-bus
  vision's D3–D6, D14 and his answers to the plan's ship-gate questions;
  `lp2025/2026-10-20-0001-playlists-on-the-bus`)
- **Supersedes:** `docs/design/modules.md` R7's "inputs need no
  counterpart" for presentation (resolution is unchanged)
- **Superseded by:** None

## Context

The pattern library says Palette and Detail stay put across a switch. In
the model they did not: all thirteen PLAYFUL tryout patterns read
`bus:palette` and `bus:detail` in their own module scope, each with its own
default, and nothing at the root named either channel. So each showed in
the playing pattern's group, remembered per entry, and changed on every
switch. A channel only exists where some binding names it (R3), and a
playlist entry's sink hides its child's channels (R2).

Sharing is already inheritance (E4): a writer at the playlist's level that
every pattern inherits, made only when someone touches it. Reads already
walk out of a sink (R5). What was missing was the listing — a control at
the root to write.

## Decision

1. **The pattern says what it takes** (D4). `module.json` gains an
   optional `inputs` list of bare channel names, declared beside `exports`
   (its mirror): `"inputs": ["palette", "detail"]`. Names only; a shared
   control takes its label and range from the declaring module's bound slot
   (R9). Opting out is not declaring.
2. **A sink passes its loaded child's declared inputs up, for reads only**
   (D5, the plan's Q5). A module's declared inputs list in its host scope —
   the scope it is placed in, lifted out of a playlist entry's sink to the
   playlist's scope (`NodeTree::input_host_scope`). One level: an input goes
   further only if the host module declares it too. Only a loaded module
   declares anything, so a dormant entry lists nothing (R2's no demand).
3. **The band is the root scope's own channels plus the playing pattern's
   declared inputs** (D6). Studio derives each shared control from the
   playing pattern's own control for the channel, retargeted to the host
   scope; at rest it shows the pattern's resolved value (its own default).
   The pattern's group shows its own control for a declared input only
   while the pattern holds its own value (D14).
4. **`detail` joins the well-known channels**, after `palette`, kind
   `Ratio` (Q7). It used to fall through the slot-name heuristic to
   `Color`.
5. **The catalog declares**: the 13 tryout modules and 13 library patterns
   take both; comet, fire2012, palette-waves, plasma and plasma-duo take
   `palette`; meteor and pulse declare nothing.
6. **No `PROJECT_FORMAT_VERSION` bump** (DP11, Yona 2026-10-20: "fine for
   now, it'll still show the firmware update being available"). An
   additive optional field, the cycle/skip precedent.

## Consequences

- Turn the band's Palette and every pattern that declares it follows; set
  one pattern's own palette and that pattern goes its own way; reset
  brings it back. Until the band is touched each pattern plays its own.
- A pattern that does not declare `palette` hides the band's control while
  it plays: the board only knows the playing pattern's inputs.
- **Old readers refuse new bytes.** The closed-vocabulary reader refuses an
  unknown key, so an older Studio, or a board kept on older firmware,
  refuses a project whose patterns declare `inputs`. Studio offers the
  firmware update in that case.
- A small authoring burden: a pattern's author lists its shared knobs.
  Yona: "about the right amount".

## Alternatives Considered

- **The piece declares its shared knobs** on its root module, with Studio
  filling the list on import. Rejected: the pattern's author is the one who
  knows whether a knob means the same thing in every pattern, and the list
  would have to be kept in step with the patterns.
- **The bus registry marks channels as piece-wide.** Rejected: hard-coding,
  which Yona asked to avoid.

## Follow-ups

- Per-pattern configs: a stop that ships with its own knob values (D16).
