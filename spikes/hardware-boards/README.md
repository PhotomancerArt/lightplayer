# Hardware / board selection — UX spike

Raw-HTML exploration (repo spike convention) for board selection as a first-class
feature. Feature notes: Dropbox `Planning/lp2025/_features/hardware - selection, pin discovery.md`.

Open `index.html` (any static server). **Rev 2** after Yona's gate feedback.

## The row layout engine (rev 2 core)

The pin row is the layout unit: `u` = pin pitch = row height, and everything
derives from it (pad `0.6u`, cell `0.78u`, font `0.5u`). Concepts:

- **Row** — one pin's horizontal band; cells never leave it, so *every* pin can
  hold cells simultaneously (rev 1's staggered callouts could not).
- **Cell** — typed chip in a row: name, capability (`adc`/`touch`/`spi`/`i2c`/
  `uart`/`usb`/`strap`/`warn`), connection (violet = bound), or color swatch.
- **Rail** — the stack of rows on a board edge (left/right extend outward).
- **Band + Leader** — top/bottom-edge pins (e.g. QuinLED screw terminals) get
  rows in a band above/below the rails, tied to the pad by an elbow leader line.

One renderer, four modes: `plain` (catalog thumbnails), `caps` (what's
supported — the classic pinout-diagram view), `wired` (what's connected, device
card), `swatch` (pin discovery). Board defs are plain JSON — agent-buildable
(this page's were), user-editable later.

## Surfaces

1. **Boards catalog** — cards w/ SVG wireframe, gold/silver/bronze tier, price,
   purchase links, sort + SoC filter.
2. **Board pinout** — full capability view per board + the design-language doc.
3. **Provisioning picker** — flasher-detected SoC leads; mismatches collapse;
   dashed generic fallback.
4. **Device card · hardware pane** — wired connections inline in pin rows;
   QuinLED demos the top-edge terminal band.
5. **Pin discovery** — solid color codes (no motion). Step 1: color-order test
   (K-R-G-B-K-B-G-R-K on every pin; user picks the row they see → RGB/GRB/…).
   Step 2: combinatoric codes — smallest pixels×colors from RGBCMYW covering the
   free-pin count, computed in code (e.g. 20 pins → 2 px × 5 colors). Input-only,
   USB-JTAG (C6 IO12/13), and in-use pins are auto-skipped.
