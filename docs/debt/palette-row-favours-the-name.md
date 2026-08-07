---
status: carried
since: 2026-08-06
logged: 2026-08-06
area: lp-app/lpa-studio-web/src/app/node/panel/palette_chooser.rs (PaletteRow)
related:
  - docs/adr/2026-08-04-palette-catalog-licensing-and-isolation.md
  - lp-app/lpa-studio-web/src/app/node/panel/palette_catalog.rs
---
# The palette chooser's row gives the gradient a chip and the name the room

**Shape** — a catalog row is `[ strip ] [ name over dim "author · SPDX" ] [ + ]`
inside a 300 px popover, so of ~280 px usable the gradient takes 96 px and its
label takes the rest. It was 56 px (20 %) until the M4 follow-up widened it to
96 px as a stopgap. The proportion is still backwards for what the list is
*for*: the chooser has a search box, so finding a palette by name is a typing
task, and the list's real job is visual scanning and comparison — which is the
one thing the layout gives least space to.

**Why it is acceptable now** — 96 px is enough to read a ramp's character
(where the dark run sits, whether it wraps, roughly how many bands), the names
still fit unelided at the catalog's longest ("Rainbow Stripes",
"Blackheartedwolf 01"), and nothing is misrepresented. It is a proportion
complaint, not a correctness one.

**What the fix probably looks like** — Yona's sketch is a full-width gradient
with the name and credit on a small line beneath:

```text
[ ────────────── gradient ────────────── ]   [+]
Blackheartedwolf   Blackheartedwolf · CC-BY-3.0
```

Three things a proper exploration has to settle, none of which are obvious:

- **Row height is the currency.** A full-width strip plus a text line lands
  around 40 px/row, so the `max-h-[264px]` list shows ~6 rows instead of ~9.
  "Give the gradient full width" and "see enough palettes to compare" are in
  direct tension, and the trade has to be judged on a rendered LIST, not on
  one handsome row.
- **Text on the gradient is out.** Legibility over an arbitrary ramp needs a
  scrim, and the scrim dims exactly the colors being judged — failing worst on
  the dark palettes that most need reading. Ruled out by inspection, not by
  taste alone.
- **The credit is often redundant.** cpt-city palettes are named after their
  author ("Blackheartedwolf 01", author "Blackheartedwolf"), so the second
  line says the word twice. Suppressing the author when it is already a prefix
  of the name frees the width that may let name and credit share ONE line —
  which is what would make the sketch fit. FastLED rows ("Cloud", author
  "FastLED") do not have that property, so both cases must appear in any
  sample list used to judge a layout.

A variant worth including beyond the sketch: full-width strip with the text
revealed only on hover/selection — standing state is pure color at full width
and ~9 rows stay visible, at the cost of not being able to read every name at
once (which the search box arguably already covers).

**How we would know it is paid** — a `yona-ux` spike under `spikes/` comparing
the concepts as 8–10 row lists inside a 300 × 264 scroll box, with the
rows-visible count stated per concept, converged at a visual gate; then the
winner rebuilt as `studio__node__palette-chooser__*` stories. Deliberately
deferred to a Mythos-class model: concept generation and dense hand-rolled CSS
are where model tiers separate hardest, and a weaker exploration would bank a
worse answer than no answer.
