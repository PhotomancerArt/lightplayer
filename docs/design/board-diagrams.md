# Board diagram design language

Vocabulary for LightPlayer board diagrams. Every board drawing renders from
metadata (`boards/<vendor>/<product>.display.json` sidecars, see
`lp-app/lpa-boards/`) through one SVG renderer: the `BoardDiagram` Dioxus
component (`lpa-boards/src/diagram.rs`), whose geometry lives entirely in
`lpa-boards/src/geometry.rs`. There are no hand-drawn board images.

Ported from the approved UX spike (`spikes/hardware-boards/DESIGN-LANGUAGE.md`
rev 5, PR #222, two visual gates).

## Layout vocabulary

| Term | Meaning |
|---|---|
| **Unit** `u` | Pin pitch = row height. THE scaling factor — every other dimension derives from it. |
| **Row** | One pin's horizontal band, `u` tall. All annotation for a pin lives in its row, so every pin can be annotated simultaneously. |
| **Cell** | A typed chip inside a row. Width fits content; height is fixed (`0.78u`). |
| **Rail** | The stack of rows on a left/right board edge. Rails extend outward from the pads. |
| **Band** | Horizontal strip above/below the rails holding rows for top/bottom-edge pins (screw terminals etc.), which can't have vertical rows. |
| **Leader** | Elbow line connecting a top/bottom pad to its band row. |
| **Pad** | The pin's physical marker on the board edge, colored by role. Two styles via `pad_style`: the flat header/solder pad (default) and the **screw terminal** — a square block with a screw head, matching the band's terminal drawing, for boards whose rail pins are screw blocks (DIN-rail controllers like the DOM-Z-102). |
| **Label** | The pin's name, silkscreen-style *inside* the board edge, colored by role. Keeps outside cells left-aligned into columns. |

The annotated-anatomy story (`studio/boards/board-diagram/anatomy` in the
story book) draws this vocabulary on a real board; its callout anchors are
computed from the same `BoardLayout` the renderer draws, so the figure tracks
the engine.

## Derived geometry (all from `u`)

Constants live in `lpa-boards/src/geometry.rs` — that module is the single
home for these numbers.

| Element | Size |
|---|---|
| Row height / pin pitch | `1.00u` |
| Cell height | `0.78u` |
| Pad (header) | `0.62u × 0.45u` |
| Pad (screw) | `0.62u × 0.62u` square + screw circle `r = 0.34 ×` pad width |
| Cell font | `0.50u` |
| Cell gap | `0.22u` |

The spike settled on `u = 12` for compact contexts (device card, discovery)
and `u = 13` for detail contexts (boards page pinout). `scale` multiplies the
rendered SVG size only — geometry never changes with it.

## Cell types

Colors ride `lpb-cell--*` classes in the studio stylesheet
(`lpa-studio-web/src/style.css`), mapped onto studio palette families where
one exists.

| Type | Color family | Used for |
|---|---|---|
| `name` | slate | Pin name (band rows only; rails put names inside the board) |
| `pwr` / `gnd` | red (status-error) / dim | Power and ground |
| `adc` `dac` `touch` | green (status-good) / blue (status-live) / orange (status-attention) | Analog capabilities |
| `spi` `i2c` `uart` `usb` | violet / cyan / brown / amber | Buses |
| `strap` | amber | Boot-strap pins — usable, with care |
| `warn` | dim orange | Caution: input-only, XTAL, PSRAM-reserved, JTAG |
| `conn` | **violet** (status-bound) | A bound connection (studio convention: bound = violet, never green) |
| swatch | — | Discovery color code (n solid squares) |

## Pin roles (pad + label color, drives eligibility)

`pwr5` `pwr3` `gnd` `io` `ioin` (input-only) `strap` `usb` `ctl` (EN/RST)
`nc` `rsvd` (in-package flash/PSRAM — present but never claimable)

Output-eligible for LEDs: `io`, `strap` (`PinRole::output_eligible()`).
Never: `ioin`, `usb` (e.g. C6 IO12/13 USB-JTAG), `nc`, `rsvd`, power/ctl,
and pins already bound or reserved (onboard RGB).

## Renderer modes

| Mode | Question it answers | Where |
|---|---|---|
| `plain` | what does the board look like? | catalog / picker thumbnails |
| `caps` | what's *supported* on each pin? | boards page detail |
| `wired` | what's *connected*? | device card hardware pane |
| `swatch` | which pin are my LEDs on? | pin discovery |

## Discovery language

- **Order test**: the 9-pixel palindrome `K-R-G-B-W-B-G-R-K`, repeated on every
  free pin. K bounds each repeat, W marks the center; palindrome ⇒ direction
  doesn't matter; K/W are permutation-invariant ⇒ the perceived row uniquely
  identifies the strip's color order (RGB/GRB/…).
- **Code**: a pin's steady color sequence. Codes are **palindromes** — data
  direction is unknown, so a code and its reverse are indistinguishable; a
  palindrome reads the same from either end. Displayed with **K (off) pixels
  between digits** so color runs read unambiguously, but no leading/trailing K —
  short strips (even 3 LEDs) must still show 1-digit codes. A `d`-digit code
  occupies `2d−1` physical pixels.
- **Code plan**: smallest `d digits × c colors` with `c^ceil(d/2) ≥ free pins`
  (a length-`d` palindrome has `ceil(d/2)` free digits), colors drawn in order
  from the palette `R G B C M Y W` (K is reserved as the separator). Even digit
  counts are never optimal, so the ladder is 1 digit (1 px) → 3 digits (5 px) →
  5 digits (9 px). E.g. 20 pins → `3 digits × 5 colors` = 25 `X·Y·X` codes on
  5 pixels.

The discovery *mechanics* (driver rotation, channel budgets) are M7's plan;
this section defines only the visual language the `swatch` mode renders.
