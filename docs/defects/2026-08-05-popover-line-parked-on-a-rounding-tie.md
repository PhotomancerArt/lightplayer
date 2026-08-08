---
status: fixed         # diagnosed 2026-08-05; fixes (1)+(2) landed 2026-08-08
found: 2026-08-05     # how: ci (two consecutive validate-stories captures of the same tree)
fixed: 1f253f62d
area: lpa-studio-web/src/base/popover.rs + lpa-studio-web/src/style.css (.ux-node-ui-status-popup-error-detail)
class: metastable-rounding-boundary
related:
  - ../debt/story-capture-pipeline.md
  - 2026-07-27-code-editor-gutter-misaligned.md
  - 2026-07-27-story-check-tolerance-ignores-amplitude.md
  - 2026-07-26-popover-outline-stale-on-content-resize.md
---
# One popover text line is parked on a half-pixel, so a 0.1px position wobble moves it a whole pixel

**Symptom** — `exploration/node-ui/status-indicators` at `sm` captured
byte-identically in run 31024986361 and then, minutes later on the same
branch with no app change touching the story, drifted in run 31026385720:

```
exploration__node-ui__status-indicators__sm.png
  304/352560 px (0.086%) exceed Δ64 [513 any-diff, max Δ223]
```

Over the ratio limit, so the auto-commit refreshed it as 75e931304 on
`claude/musing-brattain-8fb80e` (pre-refresh bytes in its parent,
6e694f210). Classic bistable signature: two reachable settled states,
both surviving the stable pair.

## What actually differs (measured before theorising)

Diffing the two committed variants:

- The **entire** 390×904 frame is byte-identical except an 11-row band,
  `y 571–581`, `x 51–193`.
- That band is **one line** of the five-line rustc-style block in the
  *error* node's status popover:

  ```
  error[E_SHADER]: failed to compile rainbow.glsl
    --> rainbow.glsl:18:14            <-- this line, and only this line
     |
  18 | color = sample(uv2);
     |              ^^^ unknown identifier `uv2`
  ```

- Best per-line vertical alignment between the variants:

  | line | best dy | residual nonzero px |
  |------|---------|---------------------|
  | `error[E_SHADER]: …` | +0 | **0** |
  | `  --> rainbow.glsl:18:14` | **+1** | 3 |
  | `   \|` | +0 | **0** |
  | `18 \| color = sample(uv2);` | +0 | **0** |
  | `   \|              ^^^ …` | +0 | **0** |

So it is a **pure integer translation of one line by one device pixel**.
The glyphs are pixel-identical — same font, same rasterization, same
horizontal subpixel phase. Nothing reflowed: the lines above and below
did not move, so the gap above that line shrank by 1px and the gap below
grew by 1px.

This rules out, by measurement rather than argument:

- **the stale-canvas-backing mechanism** just fixed for the clock face —
  this story mounts no `ux-box-sized-canvas` (or any canvas);
- **a webfont / fallback-metrics race** — different metrics change glyph
  shapes and advance widths; these glyphs are bit-identical;
- **AA / raster jitter** (the `version-badge` and `shader-face` churner
  class, max Δ2–6) — max Δ223 is a glyph moving, an order of magnitude
  and a half above that class;
- **a mid-flight CSS colour transition** — that is wide, faint and
  wrong-shaped; this is narrow, saturated, and geometric.

## Root cause

Two things compose.

**(1) The block is laid out on a fractional line grid, and one line is
permanently parked on a rounding tie.**
`.ux-node-ui-status-popup-error-detail` (style.css:1012) sets:

```css
font-size: 0.68rem;    /* → 10.88px  */
line-height: 1.45;     /* → 15.776px */
```

Measured in Chrome, the used line pitch is **15.765625px** (= 1009/64,
i.e. `line-height` snapped to the LayoutUnit grid). Successive baselines
therefore differ in fractional part by **0.765625**, so the five lines
never share a rounding phase — their sub-pixel offsets tile the interval.
Chrome snaps a horizontal text baseline to a whole device pixel, so a
sub-pixel move of the block flips *only* whichever line happens to sit
within that move of a `.5` boundary. With five lines spaced 0.766 apart,
there is nearly always one.

Measured on the real story, the five line tops land at fractional parts
`[.1875, .9531, .7188, .4844, .25]` — the fourth is **1/64 px** from the
tie. In the CI capture it was the second line that was parked there.

**(2) The popover emits its position at one-tenth-pixel resolution, and
that position is not stable run to run.**
`PopoverPosition::style()` (popover.rs:1536) formats:

```rust
format!("left: {:.1}px; top: {:.1}px; visibility: {visibility};", self.left, self.top)
```

The panel top is derived from an async `get_client_rect()` on the trigger
(`anchor.y + anchor.height - border`), re-measured by
`measure_trigger_with_stabilization` at 50ms and 250ms, again after
`document.fonts.ready`, and again from a `ResizeObserver` — then
quantized to 0.1px. On the real story: `panel.style.top = "921.2px"` →
actual `921.1875`.

**0.1px is exactly the step that flips a parked line.** Reproduced
directly, with the same font and the same CSS, by moving a container's
top from `100.7px` to `100.8px`:

```
top 100.7  line tops [111.6875, 127.4531, 143.2188, 158.9844, 174.75  ]
top 100.8  line tops [111.7969, 127.5625, 143.3281, 159.0938, 174.8594]
                                ^^^^^^^^ 127.4531→127, 127.5625→128
```

Screenshot diff of those two renders: **644 any-diff px, 368 exceeding
Δ64, max Δ231**, confined to that one line, every other line residual 0 —
the same shape and amplitude as CI's 513 / 304 / Δ223.

And the panel top genuinely wobbles: **10 consecutive loads of the real
story in one headless Chrome emitted `921.2px` nine times and `920.2px`
once** — same build, same browser, same machine, same page.

So the story is structurally bistable: a popover whose position is
recomputed from async DOM measurements and emitted on a 0.1px grid,
wrapping a text block that always has a line sitting on a rounding
boundary. Any run-to-run wobble ≥0.1px in the popover position moves that
line a whole pixel and nothing else — which is precisely the diff CI
committed.

**Not yet identified:** *why* the underlying trigger measurement varies
between loads. The 1px case above (920.2 vs 921.2) is larger than what CI
captured (CI's variation was sub-pixel — a 1px panel move would have
shifted all five lines, not one). That wobble is its own open question.

## Fix (1 and 2 landed 2026-08-08)

Both landed together: `PopoverPosition::from_anchor` snaps `left`/`top`
to the device-pixel grid at creation (so the emitted style, the animated
outline's final rect, and the clip inset all read the same
whole-device-pixel geometry), `open_trigger_style` snaps the top-layer
trigger copy's position (rounding) and size (ceiling, so the copy can
never re-wrap a fraction narrower than the in-flow button), and
`.ux-node-ui-status-popup-error-detail` / `.ux-node-ui-json` get an
integral `line-height: 16px`. The trigger-copy half also explains the
`studio__home__new-project-menu__menu-open__sm` flap seen on in-flight
branches: the "New" trigger's glyphs re-render in the top layer at the
measured 0.1px-quantized position, so they inherit the same wobble the
panel did. (3) — why the underlying measurement wobbles — remains open,
but (1)+(2) make renders insensitive to it below half a device pixel.

The original ranking, kept for the record — deliberately not "raise the
tolerance", the debt entry's exit criterion (5b) rules that out
explicitly:

1. **Emit whole-pixel popover positions** (`{:.0}`, or round before
   formatting, in `PopoverPosition::style()`). This collapses the
   sensitivity class for *every* popover story at once: the panel's
   content box always starts on a device pixel, so each descendant's
   sub-pixel phase is fixed by CSS alone and any measurement jitter below
   half a pixel changes nothing at all. Needs a check that welding
   (`snap_to_trigger_edges`, the outline path) still lines up when the
   panel is pixel-snapped but the trigger is not.
2. **Give the error block an integral line box** (`line-height: 16px`
   rather than `1.45` on a 10.88px font). All five baselines then share
   one rounding phase, so a sub-pixel move flips all of them or none —
   which removes the *guarantee* that some line is parked on a tie,
   though it does not make the block immune.
3. **Diagnose the trigger-measurement wobble itself.** (1) and (2) make
   the render insensitive to it; neither explains it.

(1) and (2) are independent and both worth doing — (1) removes the
perturbation's reach, (2) removes the knife edge.

## Side observation, unresolved

This story's ready gate does **not** converge on macOS Chrome 142: three
`[data-story-wait="1"]` elements never clear, and
`studio-story-pngs.mjs` times out at 30s and retries on a fresh page, on
every attempt. CI's pinned Chrome 151 captures the story fine, so this is
either environment-specific or a latent stabilization bug that CI happens
to survive. It is why the flip could not be reproduced end-to-end through
the real harness locally — the mechanism above was proven with a direct
CDP probe instead.
