# Hardware / board selection — UX spike

Raw-HTML exploration (repo spike convention) for board selection as a first-class
feature. Feature notes: Dropbox `Planning/lp2025/_features/hardware - selection, pin discovery.md`.

Open `index.html` (any static server). Four surfaces, one shared idea: a board is
**metadata**, and one SVG renderer draws it everywhere.

1. **Boards catalog** — the public "what should I buy" page. Cards w/ SVG wireframe,
   soc/flash/caps, price, purchase links, gold/silver/bronze support tier. Sort + filter.
2. **Provisioning picker** — we detected a SoC; show matching boards first, mismatches
   collapsed, generic-board fallback for unknown hardware.
3. **Device card · hardware pane** — board wireframe with callout boxes on the wired
   pins (LED outputs, buttons), driven by the project's output config.
4. **Pin discovery ("LED finder")** — drive a unique color/blink pattern on every free
   data-capable pin; user clicks the pattern they see on their strip. Input-only and
   USB-JTAG pins (C6 GPIO12/13!) are excluded.

Decisions this spike proposes (gate questions in the handoff):
- Board metadata schema (see the collapsible JSON in the page) — name/mfr/urls/price,
  soc, flash/psram, caps list, tier, and a `hw` drawing block (module, usb, buttons,
  headers w/ per-pin role). Pin roles: pwr/gnd/io/input-only/strap/usb/ctl.
- Support tiers: gold = first-class + tested every release, silver = tested
  occasionally, bronze = community / should work.
- Violet = pin bound to an output (studio convention: bound = violet, never green).
- Discovery patterns = color × blink-count (+chase), so patterns stay distinguishable
  for colorblind users via rhythm, not hue alone.
