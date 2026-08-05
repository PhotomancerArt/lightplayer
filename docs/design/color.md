# Color — LightPlayer color strategy

This document is the **single source of truth** for color representation,
precision, and conversion in LightPlayer. All implementations
(lpc-model, lpc-engine, lpfx, fixture/output drivers, debug viewers,
shader codegen) must conform.

## TL;DR

1. **The canonical numeric space is linear-light sRGB primaries
   (`LinearSrgb`), F32 per channel, RGB only.** All shader uniforms,
   bus channels, gradient interpolation, and engine math operate
   here.
2. **Color values carry their authoring colorspace as a struct
   field** (`space`). The picker's intent round-trips. The loader
   converts to canonical at the binding boundary; shaders never see
   authoring spaces.
3. **8-bit linear is forbidden.** `Unorm8` always means
   display-encoded sRGB in engine. Only fixtures whose native output format is 8-bit linear (e.g. WS2811) use this.
4. **Overshoot (>1.0) is allowed in scalars and color values.**
   Out-of-gamut conversions and "boost" multipliers are real and
   useful. The framework does not silently clamp.
5. **The lossy clamp happens at exactly one boundary**: writing to a
   Unorm16 texture. Tone-mapping is future-additive at the same
   boundary.

## 1. Why this matters

LED art is unusually sensitive to color precision and pipeline
choices, for three reasons that compound:

- **LEDs are linear-PWM by nature.** A 50% duty cycle gives 50% of
  the photons, but humans perceive that as much brighter than half.
  Gamma correction is required _somewhere_ to make pickers and math
  match perception.
- **8-bit linear is broken.** sRGB encoding exists precisely to make
  8 bits perceptually usable; its non-linearity is the _whole point_.
  A `u8` linear color wastes ~95% of its codes in highlights and
  bands visibly in shadows. We never want to be in this trap.
- **Picker round-trip is a UX expectation.** A user who picks a
  color in OKLCH expects to see it in OKLCH the next time they open
  the file. Lossy normalization to a canonical authoring space (e.g.
  always-Srgb storage) destroys intent.

The strategy below resolves all three by being explicit about
_where_ each transformation happens.

## 2. Pipeline contract

The complete per-layer contract. Every implementation must conform to
the format and range columns at each boundary.

| Layer                                      | Format                                                         | Range                             |
| ------------------------------------------ | -------------------------------------------------------------- | --------------------------------- |
| Authored `coords` in TOML / `LpsValue`     | F32 per channel + `space` tag                                  | unbounded F32                     |
| Color values in `LinearSrgb` (canonical)   | F32 per channel                                                | unbounded (overshoot OK)          |
| Shader uniforms (after binding)            | F32 per channel, `LinearSrgb`                                  | unbounded                         |
| Param scalars (`Amplitude`, `Ratio`, etc.) | F32 (or Q16.16 fallback)                                       | per-Constraint, default unbounded |
| Engine internal accumulation (lp-engine)   | F32 default; **Q16.16 ([Q32](./q32.md)) on int-only firmware** | overshoot expected                |
| Texture-write boundary (shader → texture)  | **Unorm16 linear** — `vec3` clamped at write                   | clamped to **[0, 1]**             |
| Texture sample (fixture reads texture)     | F32 from Unorm16                                               | [0, 1]                            |
| Output device → hardware                   | device-determined (`Unorm8` sRGB, raw PWM, etc.)               | device-determined                 |

### Default vs fallback numeric type

LightPlayer leans toward float-capable hardware. The default
representation for canonical color and accumulation is **F32**.

On int-only firmware variants, **Q16.16 fixed-point ([Q32](./q32.md))**
is the authorized fallback. The contract is identical — only the
representation changes. Code that needs to work in both modes uses
the engine's numeric abstraction.

## 3. Canonical: `LinearSrgb` F32, RGB only

The bus, all shader uniforms, and engine math all operate in
**linear-light, sRGB primaries, F32 per channel, RGB (3 channels)**.
This is the single point of truth a stage downstream can rely on.

Why these specific choices:

- **Linear-light**: required for additive blending, fluid sims,
  alpha compositing, and any lighting math to be physically sound.
  Mixing two pure-red and pure-green at 50/50 in a non-linear space
  produces a muddy desaturated yellow; in linear it's the correct
  bright yellow.
- **sRGB primaries**: the de-facto standard. All authoring spaces
  ultimately convert through these. Wider-gamut primaries (Display
  P3, Rec.2020) are future-additive when a real use case appears.
- **F32**: per the precision contract, integer-encoded linear color
  is broken. F32 is the cheapest unambiguous representation on
  float-capable hardware.
- **RGB (3 channels)**: graphics is inherently RGB at the math
  layer. RGBW, RGBAW, and arbitrary multi-primary hardware is
  handled at the **fixture/output** stage, where the additional
  channel is _synthesized_ from the canonical RGB. It is never
  authored as a 4th color channel here. Native multi-primary
  authoring would be a new feature, not a default.

## 4. Authoring colorspaces

What the user picks in. Stored in the value's `space` field for
round-trip; converted to canonical at binding.

| Variant      | Notes                                                                                                    |
| ------------ | -------------------------------------------------------------------------------------------------------- |
| `LinearSrgb` | Linear-light, sRGB primaries. The canonical numeric space. No-op when authored.                          |
| `Srgb`       | Display-encoded sRGB (gamma ~2.2). What HTML hex colors and most pickers emit.                           |
| `Hsl`        | Hue / Saturation / Lightness. Cylindrical reparametrization of sRGB. Popular picker.                     |
| `Hsv`        | Hue / Saturation / Value (a.k.a. HSB). Cylindrical reparametrization of sRGB. Popular for "rainbow" art. |
| `Oklab`      | Perceptually uniform Cartesian. Best space for color math (averaging, gradient interp).                  |
| `Oklch`      | Perceptually uniform cylindrical (polar Oklab). Best modern picker for hue manipulation.                 |

Notes:

- We deliberately don't expose a bare `Rgb` — it's ambiguous (which
  primaries? which gamma?). `Srgb` is the de-facto authoring choice;
  `LinearSrgb` is the canonical math choice.
- All variants store F32 `coords`. `Unorm8` is reserved for
  display-encoded _output_ and never appears as a colorspace storage
  option here.
- **Hue is degrees; everything else is a `0..1` fraction.** The
  cylindrical spaces (`Hsl`, `Hsv`, `Oklch`) carry their hue angle in
  degrees, matching CSS (`hsl(120deg …)`, `oklch(… 120)`) — which is
  what a picker, a pasted CSS value, and every palette-import source
  already speak. Nothing else in a `coords` triple is an angle, so the
  whole rule is "the only unit that is not a fraction is a hue, and it
  is degrees". Hues wrap; `Oklab`'s `a`/`b` and `Oklch`'s `C` are
  signed/unbounded coordinates, not fractions. The lane-by-lane table
  lives with the conversions
  (`lp-core/lpc-engine/src/color/colorspace.rs`).
- The integer encoding is a Rust enum `repr(i32)`. Stable values
  matter (it's serialized into `LpsValue`); reserve room
  (`LinearSrgb = 0`, others incrementing) and never renumber.
- Wider-gamut spaces (Display P3, Rec.2020, ACES) and CIE Lab/LCH
  are out for v0; additive later when a use case appears.

## 5. Color-family kinds

Two Kinds in the color family. Each is a struct shape with the
colorspace and color data inside the value. These structs are the
authoring/storage model, not necessarily the shader ABI for every
runtime path.

| Kind       | Storage (`LpsType::Struct`)                                                           |
| ---------- | ------------------------------------------------------------------------------------- |
| `Color`    | `{ space: I32, coords: Vec3 }`                                                        |
| `Gradient` | `{ space: I32, method: I32, count: I32, stops: Array(Struct{at: F32, c: Vec3}, 24) }` |

There is **no separate `ColorPalette` Kind**: a discrete swatch list
is a `Gradient` with `method: Step` (D2 of the palette spike,
`2026-08-03-1803-palette-exploration-spike`). One collection type, one
bake path, one editor.

Conventions:

- `space` is `Colorspace as i32` (per the table above). Stable repr.
- `method` is `InterpMethod as i32` (see §6).
- Both encoded as integers in storage so `LpsValue` stays purely
  structural; the loader maps authored strings (`"oklch"`,
  `"linear_srgb"`, `"srgb"`, ...) to the integer encoding.
- **`coords` and `stops.c` are F32** per the precision contract.
- **A gradient shares one space at the collection level.** A
  `Gradient` carries a single `space` for all stops — never
  per-element. Same for `method`.

### Fixed-size arrays + explicit count

`Gradient` uses a **fixed-size array** sized by a constant:

```rust
pub const MAX_GRADIENT_STOPS: u32 = 24;
```

24 rather than the original 16: WLED gradients carry up to 18 stops
and importing one must not truncate (D2 of the palette spike).

The fixed size is the **type's maximum**, and an explicit `count: i32`
field carries the number of authored entries; consumers iterate
`0..count`. No sentinel values; no ambiguity.

**Stored values are count-bounded, not padded** (amended 2026-08-05).
The `LpValue` form carries exactly `count` array entries; readers
accept any length in `count..=MAX` (the legacy zero-padded form still
decodes; entries past `count` are never read). The original
always-maximum-size rule put ~17.7 KiB of dead padding on every wire
crossing — larger than the whole 16 KiB project-read frame budget by
itself, which broke project sync the moment a palette pick landed a
config on a bus channel (probe value echo) or in a def (slot-root
echo). `lp-core/lpc-shared/tests/gradient_wire_size.rs` pins the
count-bounded sizes; the residual maximal-cycle bound is
`docs/debt/maximal-gradient-cycle-exceeds-frame.md`. Generically, the
slot machinery treats a fixed `Array(N)` type as accepting **up to**
`N` elements (codec read/write and `lp_value_matches_type`); the
absent tail is type-default. A GPU-facing fixed layout, where one is
ever needed, pads at materialization — gradients themselves never take
that path (they bake to textures, below).

Authored values _exceeding_ the maximum are a load error, not
silently truncated. Larger collections are a one-constant bump in
v1+, not a model change.

The authored JSON surface is **not** this fixed shape: it carries
snake-case enum strings and exactly the stops that were authored.
`lpc-model`'s `color` module owns both encodings and the conversion
between them; the fixed shape is what `LpValue` / `LpType` and the
GPU layout logic see.

**Except in node-def JSON** (M4-P5 decision): a shader slot's inline
`gradient` option is read by the shape-driven slot codec — a streaming
reader guided by `LpType`, with no serde bridge — so what `shader.json`
can spell there is the fixed recipe: integer tags, `count`, and a
count-bounded (or legacy zero-padded) `set`/`stops` array. Teaching
that codec the friendly form was considered and declined: it would
plant a per-type special case keyed on struct name inside
shape-generic machinery, well past the "small, in one place" bar the
loader-maps-TOML-strings precedent sets. **The fixed recipe is the
authored contract for inline gradient configs** — friendly enum
strings and per-variant shapes do not belong there. In practice nobody
hand-writes one: Studio's chooser (M4) writes configs as `LpValue`
through `SetValue`, an unauthored palette slot falls back to
`gradient_config()`'s default, and the friendly surface stays where
serde actually reads it — the palette catalog's TOML
(`lpa-palettes::entry`) and other serde-facing surfaces.

For lpfx rendering, `Gradient` values materialize to width-by-one
texture resources before shader binding. Shaders sample those
resources as `sampler2D` uniforms using the lp-shader
`TextureShapeHint::HeightOne` contract. This keeps color authoring
textual and structured while avoiding fixed-size gradient uniform
structs as the shader-facing ABI.

**The shipped strip** (`lp-core/lpc-engine/src/color/gradient_bake.rs`)
is **256 × 1 `Rgba16Unorm`, sampled `filter=linear wrap=repeat`**, and
texel `i` holds the gradient at `t = (i + 0.5) / 256` — the texel
*center*, so `texture(palette, vec2(u, 0))` returns the gradient at `u`
exactly rather than half a texel off. `Repeat` is deliberate: a palette
read past its end wraps, so a shader can scroll `u` without clamping
and a gradient authored to wrap joins itself. The seam that follows
from those two facts (`u = 0` is the midpoint of the last and first
texels, not the first texel) is pinned by
`lp-shader/lps-filetests/filetests/texture/palette_strip_*.glsl` on
both numeric tiers.

The engine supplies the matching compile-time `TextureBindingSpec` for
every `palette`-kind shader slot; without it a `sampler2D` uniform does
not compile at all. See
[`lp-shader-texture-access.md`](./lp-shader-texture-access.md).

## 6. Gradient interpolation

| Variant  | Notes                                                                           |
| -------- | ------------------------------------------------------------------------------- |
| `Step`   | No interpolation; sample picks the nearest stop ≤ `t`. Hard-edged palettes.     |
| `Linear` | Linear interpolation between adjacent stops in the gradient's declared `space`. |
| `Smooth` | Smoothstep / cubic interpolation. Softer transitions.                           |

**Interpolation happens in the gradient's declared `space`**, not in
canonical. This is the entire point of letting users author gradients
in non-linear spaces — an OKLCH-interpolated rainbow wraps correctly
through the hue, an sRGB-interpolated one does not. After
interpolation, the result converts to canonical for the shader.

Stable repr:

```rust
#[repr(i32)]
pub enum InterpMethod {
    Step    = 0,
    Linear  = 1,
    Smooth  = 2,
}
```

## 7. Conversions — where and when

All colorspace conversion is the **loader's** and **engine's**
responsibility, not the shader's. Shaders only ever see canonical
F32 LinearSrgb.

| Conversion                                          | Where                                              |
| --------------------------------------------------- | -------------------------------------------------- |
| Authoring space → `LinearSrgb` (single color)       | At uniform-binding time. Once per param per frame. |
| Authoring space → `LinearSrgb` (palette entries)    | At texture-bake time. All entries in one pass.     |
| Authoring space → `LinearSrgb` (gradient sample)    | At texture-bake time, after interpolating in the   |
|                                                     | gradient's authoring space.                        |
| `LinearSrgb` F32 → Unorm16 linear                   | At texture-write boundary (shader output).         |
| Unorm16 linear → F32                                | At texture sample (fixture / next stage).          |
| `LinearSrgb` F32 → fixture control samples          | At the fixture boundary, including LED gamma       |
|                                                     | correction when enabled for that fixture.          |
| Fixture control samples → output transport          | At the output device boundary: interpolation,      |
|                                                     | dithering, global brightness, and transport pack.  |

The gradient rows are implemented in
`lp-core/lpc-engine/src/color/`: `colorspace.rs` interpolates in the
authored space and converts the result, `gradient_bake.rs` writes the
texels. **The Unorm16 write is the only clamp in the palette path** —
out-of-gamut and boosted coordinates (§10 rule 6) survive
interpolation and conversion intact and give up their range exactly
once, at the storage boundary that cannot hold them.

A cross-fade between two palettes blends in **canonical LinearSrgb**,
after each side has been interpolated and converted in its own space.
That is the only well-defined choice when the two sides disagree about
authoring space — which a cycle's set routinely does — and it is what
a dissolve means physically.

## 8. Output stage responsibilities

The fixture / output device layer owns _all_ hardware-specific
color work:

- **Final gamma correction lives on the fixture.** LEDs are
  linear-PWM, so the physical LED transfer curve is a fixture
  property, not an output transport property.
- **Brightness / white balance / per-fixture color correction.**
- **Channel synthesis (RGBW, RGBAW, etc.).** The W channel for an
  RGBW strip is computed from the canonical RGB the engine produced.
  Algorithm choice (max-component subtract, perceptual, etc.) is
  per-fixture configuration.
- **Output bit depth.** Unorm8 sRGB for ws2811 / DMX, raw PWM for
  GPIO, whatever Art-Net / OPC / e131 need. Fixture-level concern.

The engine guarantees: **the fixture/output stage receives canonical
LinearSrgb F32 (or its int-only Q32 equivalent), 3 channels, range
[0, 1] post-clamp.** Everything fixture-specific happens after that.

## 9. Debug viewing

Two simple cases:

- **`Color` value with `space` field**: render in the authored
  `space` so the picker round-trips visually for the user. The debug
  view _is_ the picker.
- **Texture / intermediate buffer (no `space` tag)**: it's canonical
  `LinearSrgb`. The debug viewer applies sRGB encoding for display
  on a normal monitor. That's a viewer concern, not a domain
  concern.

The engine never re-tags a buffer with a colorspace; if it isn't
canonical, it isn't a buffer the engine produced.

## 10. Non-negotiable rules

The hard rules. Implementations that violate these are wrong.

1. **`Unorm8` is display-encoded sRGB or device output. Never linear.**
   If you see a `u8` color channel in `lpc-model` or `lpc-engine`
   internals, it's a bug or it's at the output stage.
2. **Canonical color is `LinearSrgb`, F32, 3 channels.** Shaders,
   uniforms, and engine math operate here. No exceptions.
3. **Color values carry their authoring colorspace.** No silent
   normalization to `Srgb` or `LinearSrgb` at storage time.
4. **The lossy clamp happens at one and only one boundary**:
   writing to a Unorm16 texture. Tone-mapping (when added) lands
   here too.
5. **No bare `Rgb` colorspace variant.** `Srgb` for display
   authoring; `LinearSrgb` for canonical math.
6. **Constraint default for color coords is `Free`, not `[0, 1]`.**
   Overshoot is meaningful (out-of-gamut, boost). A Slot that wants
   to enforce in-gamut authoring overrides with an explicit `Range`.

## 11. Future-additive (not in v0)

The contract is designed to grow without breaking these:

- **HDR textures.** When bloom / very-high-dynamic-range fluid sims
  / etc. need >1.0 in textures, `Texture` Kind grows a `precision`
  field with `F16` / `F32` variants. Until then, do HDR math in the
  engine accumulation buffer (already F32 / Q32, overshoot
  expected) and tone-map on the way to Unorm16.
- **Tone-mapping.** Reinhard, ACES, etc. — opt-in operator that
  lands at the texture-write boundary, between shader output and
  Unorm16 clamp.
- **Wider-gamut authoring.** `Display P3`, `Rec.2020`, `Aces`
  variants of `Colorspace` plus their conversion matrices. Adds new
  entries with stable enum values; doesn't change the canonical or
  the contract.
- **CIE Lab / LCH.** Older perceptual spaces. Mostly superseded by
  Oklab/Oklch but useful for compatibility with existing color
  data. Additive.
- **Larger color collections.** Bump `MAX_GRADIENT_STOPS`. One
  constant change; per-collection-instance storage size scales
  linearly.
- **Native multi-primary authoring (RGBW etc.).** Currently a
  fixture-stage concern. If a use case needs to author white
  separately at the canonical layer, that's a new Kind and a
  channel-count change — explicit, not implicit.

## 12. Reference implementations

These crates / files implement this contract:

- `lp-core/lpc-model/src/color/gradient.rs` — the `Colorspace`,
  `InterpMethod`, `MAX_GRADIENT_STOPS`, and the `Gradient` storage
  recipe (§5) plus its authored serde surface.
- `lp-core/lpc-model/src/color/gradient_config.rs` — `GradientConfig`,
  the static-or-cycle read of a palette.
- `lp-core/lpc-model/src/value/legacy_kind.rs` — the legacy `Kind`
  storage recipes; `Kind::Gradient` delegates to the module above.
- `lp-core/lpc-engine/src/color/colorspace.rs` — every authoring space
  → canonical LinearSrgb, and in-space interpolation (§4, §6, §7).
- `lp-core/lpc-engine/src/color/gradient_bake.rs` — the 256 × 1
  `Rgba16Unorm` strip and the Unorm16 write boundary (§5, §7).
- `lp-core/lpc-engine/src/nodes/shader/palette_eval.rs` — where one
  full-cycle phasor puts a `GradientConfig::Cycle`, and the mix
  quantization that bounds a fade's bakes.
- `lp-core/lpc-engine/src/nodes/shader/palette_bake_cache.rs` — strips
  keyed by the hash of the resolved value.
- `lp-shader/lpvm/src/runtime/color.rs` — colorspace conversion at
  uniform-binding time for single colors (not yet written; the palette
  path above does its own).
- Per-fixture drivers under `lp-core/lpc-engine/` — fixture-stage
  gamma, white balance, channel synthesis.

## See also

- [`q32.md`](./q32.md) — Q16.16 fixed-point semantics (int-only
  fallback for canonical color).
- [`glsl-layout.md`](./glsl-layout.md) — std430 packing rules for
  uniform / storage buffer layout (relevant for color-family struct
  storage on the GPU).
- `docs-archive/roadmaps/2026-04-22-lp-domain/notes-quantity.md` — broader
  Quantity model that color sits inside (Slot, Shape, Kind,
  Constraint, etc.).
