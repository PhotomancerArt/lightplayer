# Gradient stops are one compact string literal; metadata stays structural

- Status: accepted
- Date: 2026-08-05
- Context: amends `docs/design/color.md` §5 as set by ADR
  2026-08-04-palettes-are-values (and the palette M4 plan's P5
  "padded-form-only" decision). Found at the M4 gate: picking any
  palette in browser Studio killed project sync with "project-read
  event exceeded frame budget of 16384 bytes". No production palette
  content existed yet, making this the last cheap moment to change the
  storage form; the decision was made deliberately before PR #348 went
  ready.
- Plan: `planning/2026-08-05-2050-gradient-string-stops` (external
  planning root; decision register Q1–Q8 in its `notes.md`).

## Decision

A gradient's **metadata is structural JSON; its stop list is one
compact string literal** — and that shape is identical on every
surface: `LpValue` storage, wire, `.lp/panel.json`, authored def JSON,
catalog assets, serde. There is no second encoding.

```json
{ "space": "oklab", "method": "smooth",
  "stops": "(0.211,-0.017,-0.039) (0.425,-0.078,0.01)@.25 #f80" }
```

Grammar (`lpc-model/src/color/stops_string.rs`, specified in
color.md §5): whitespace-separated `color[@position]` tokens; colors
are `#rgb`/`#rrggbb`/`#rrrrggggbbbb` hex tiers (`[0,1]`-fraction
notation, `k/(2ⁿ−1)`) or decimal triplets `(a,b,c)`; positions are
optional CSS-style (unpositioned first → 0, last → 1, interior runs
distribute; ordering unenforced, mirroring `Gradient::validate`).
Printing is canonical and bit-exact under round-trip: positions omit
iff exactly evenly spaced; hex prints only for sRGB-shaped spaces and
exactly-`k/255` components.

Key sub-decisions (register in the plan's notes):

- **Colors are raw coordinates in the gradient's own `space`, never
  RGB-converted** (Q2). Converting would change interpolation inputs
  (the engine interpolates in `space`), drift the editor through
  decode/re-encode cycles, and re-couple what CSS correctly separates
  — notation vs interpolation space. Hex is notation for `[0,1]`
  fractions, not "an sRGB color"; the printer confines it to
  srgb/linear-srgb so a Lab gradient never looks like RGB bytes (Q3).
- **`oklch` triplets are `(L, C, H°)` — hue in degrees**, ratifying
  the convention flagged open since palette M2 (Q8).
- **`GradientConfig` stays structural**:
  `{kind, set: List(Gradient), step_seconds, fade_seconds}` —
  structure for the collection, a literal per gradient. **Both
  `count` fields are deleted**; the set's length is the count and a
  literal self-describes; limits move to parse/validate (Q5).
- **No per-value version field** (Q6): the project format floor +
  `lpa-upgrade` machinery owns shape evolution; per-value `v` costs
  embedded wire bytes forever against a need that machinery already
  covers.
- **Serde converges** (Q4): the friendly array-of-stops serde form is
  gone; catalog files carry the literal (sRGB imports as hex
  one-liners — the regeneration byte-snapped the M3 files' 6-decimal
  roundings back to the upstream sources' exact 8-bit values; Oklab
  originals as decimal triplets). A catalog test asserts every asset
  file's literal is byte-exactly canonical.
- Legacy struct-of-stops `LpValue`s no longer decode — alpha posture,
  zero prod content, no migration. The shape-generic "fixed `Array(N)`
  accepts up to `N`" codec tolerance introduced by the count-bounded
  interim stays (it is generally correct), but gradients no longer
  depend on it.

## Evidence

Measured wire sizes (tagged `LpValue` JSON via `ser_write_json_len`),
for the same configs across the three forms this branch carried:

| Config | padded (original §5) | count-bounded (interim) | stops literal |
|---|---|---|---|
| default 2-stop static | 17,682 B | 487 B | **290 B** |
| 8-stop static | ~17.7 KiB | ~1.0 KiB | **399 B** |
| cycle 4 × 8 stops | ~17.7 KiB | ~3.9 KiB | **1,148 B** |
| maximal legal 8 × 24 | ~17.7 KiB | ~21 KiB (!) | **4,368 B** |

The padded form was content-independent — EVERY config was ~17.7 KiB,
larger than the entire 16 KiB `PROJECT_READ_FRAME_MAX_BYTES` budget,
and the binding-graph probe echoes a picked channel value raw inside
one event (Studio always requests `include_values: true`), so the
first read after any pick failed the whole stream (reproduced natively
in `lpa-server/tests/panel_commands.rs`). The count-bounded interim
fixed realistic cases but still cost ~110 B/stop of tagged-JSON
scaffolding for 16 B of payload, and the maximal legal cycle
*exceeded* the frame — carried briefly as
`docs/debt/maximal-gradient-cycle-exceeds-frame.md`, retired by this
ADR (the literal is ~12 B/stop; the maximal cycle rides one frame
3.7× under budget). `lp-core/lpc-shared/tests/gradient_wire_size.rs`
pins all the sizes; the parser's 500-case generated round-trip sweep
pins losslessness.

## Alternatives considered

- **Count-bounded structured arrays only** (the interim that shipped
  for a day): fixed the realistic cases, left the maximal-cycle cliff
  and the 7× tagged-JSON overhead. Superseded here.
- **Arrays for stops (`[[at,a,b,c]…]`)**: no parser needed, but
  ~25–30 B/stop through the tagged wire encoding and still a nested
  structure on every codec surface. The string is 2× denser and ONE
  token everywhere; for the ESP32 that is fewer `ser-write-json`
  tokens out and one linear scan in.
- **Opaque binary blob (Resource/base64)**: ~16 B/stop, but gradients
  stop being readable or hand-editable anywhere, and the def codec has
  no natural home for an opaque binary leaf. Wrong altitude for a
  config value.
- **Raise the frame budget**: the budget derives the firmware serial
  scratch buffer; and with the padded form's content-independence, no
  affordable raise survived two palette channels in one probe event.
- **Generic wire compaction (RLE / shorter LpValue tags)**: hand-rolls
  the whole enum's serde for less benefit than simply not writing dead
  entries, and does nothing for distinct-stop content or def files.

## Prior art

WLED custom palettes (flat `[pos,r,g,b,…]` arrays — compact,
unreadable), FastLED `CRGBPalette16` (16 evenly spaced byte entries —
the source truth the regeneration's byte-snap restored), GIMP `.ggr`
(line-per-segment text), CSS gradients (color-stop syntax with the
interpolation space declared separately — the split this grammar
adopts).

## Consequences

- Inline def authoring becomes genuinely ergonomic — the M4-P5
  "padded-form-only, nobody hand-writes one" concession dissolves,
  since the def codec reads the same `{space, method, stops}` struct
  through an ordinary `String` leaf (no per-type teaching; the codec's
  shape-generic boundary holds).
- Catalog assets shrank ~2,300 lines; every file is one readable
  swatch line; canonicality is CI-enforced.
- The `space`/`method` snake-case tokens become the storage contract
  (token stability replaces the old repr-i32 stability for gradient
  storage; the integer reprs remain for `Color` and engine-side use).
- A maximal-content palette can no longer break project sync — the
  worst legal config is a third of a frame.
- Older-build values (padded or count-bounded structured forms) fail
  to parse post-upgrade: branch-local test state only; clear the
  panel or resave the def.
