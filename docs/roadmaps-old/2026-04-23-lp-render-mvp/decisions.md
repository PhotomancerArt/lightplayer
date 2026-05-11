# Decisions log — lp-render-mvp

This log captures the key resolved questions from the
`/roadmap` discussion that shaped this work. See `notes.md` for
the working draft and unresolved questions, `overview.md` for
synthesized rationale, and individual milestone files for
scope.

---

## D1: Defer lp-domain M4–M6, validate by use first

**Context:** lp-domain M3 finished the typed model with TOML
serde + canonical examples but never exercised the model under
load. M4 (schema codegen), M5 (migration framework), M6 (CI
gates) would lock the schemas in for the long haul.

**Decision:** Defer M4–M6 of lp-domain until after this
roadmap. Build a working visual subsystem and editor first;
locking schemas before validation is premature.

**Rationale:** Locking unvalidated shapes forces fake
migrations of our own design corpus through CI gates with no
user benefit. Better to take the churn now (no users to
disrupt) and lock later.

---

## D2: lpfx becomes the visual subsystem implementation

**Context:** lpfx today has its own parallel domain
(`FxModule`, `FxManifest`, `FxInputDef`, `FxValue`). Two
options: build a wrapper around lpfx that consumes lp-domain,
or strip lpfx in place.

**Decision:** Strip lpfx's parallel domain in place. lpfx
itself becomes the implementation of lp-domain's visual
subsystem. Long-term, lp-engine will use lpfx directly when it
composes the broader Show layer (rigs, fixtures, outputs,
signal generators, scheduling).

**Rationale:** A wrapper would mean two parallel domain
representations forever. lpfx is "brand new and not
load-bearing" per the lp-domain roadmap notes; in-place
evolution is the cheaper path. The trait surface (`FxEngine` /
`FxInstance`) is generic enough to keep, just re-typed.

---

## D3: Dioxus over Leptos

**Context:** web-demo is a wasm-bindgen shim with no UI
framework. We need a real framework for the editor.

**Decision:** Dioxus.

**Rationale:** Mobile is a strong use case (on-stage tablets,
console-side phones). Dioxus has first-class web/desktop/mobile
targets; Leptos doesn't.

**Caveat:** Dioxus mobile is WebView-based on iOS/Android
today. Web is the fast path; mobile is reversible if Dioxus
mobile turns out unworkable.

---

## D4: New crate `lp-app/lp-studio` for the editor

**Context:** Three options — `lp-app/editor` (functional name),
`lp-app/lp-studio` (long-term destination name),
`lpfx/lpfx-editor` (under lpfx).

**Decision:** `lp-app/lp-studio`.

**Rationale:** The editor in this roadmap *is* the visual
subsystem editor; long-term it grows into the full Lightplayer
Studio that composes Show / rig / fixture editors on top of
the same framework. Naming the crate after its long-term
destination avoids a rename later.

---

## D5: localStorage-backed virtual fs, seeded from examples

**Context:** Four options for browser fs — static serve,
virtual fs (`LpFsMem`), hybrid, File System Access API.

**Decision:** Virtual fs (`LpFsMem`) with localStorage
persistence and bundled-example seeding.

**Rationale:** Read-only static serve breaks down as soon as
you want to edit. localStorage gives persistence through
refresh — important for "load an example, edit it" UX. Single
active project + easy "reset to example" is enough for v0;
multi-project / FSA / server sync are additive on top.

---

## D6: Roll our own widget showcase

**Context:** Storybook-equivalent in Dioxus. Lookbook is
inactive (Sept 2024 last update); `dioxus-showcase` is one
person's experiment.

**Decision:** Roll our own. One page per widget showing useful
states, one overview page. No framework abstractions over
Dioxus.

**Rationale:** The functionality is small (widget +
hard-coded variants per page). Adopting an inactive dep adds
risk for little gain.

---

## D7: wgpu deferred to its own roadmap, anchored on
library-of-previews UX

**Context:** Initially considered including wgpu in this
roadmap.

**Decision:** Defer wgpu to a separate roadmap.

**Rationale:** wgpu doesn't add signal about whether the
domain model is right; lpvm covers device + browser + Dioxus
mobile-via-WebView already. The real motivation for wgpu is the
**library-of-previews UX** — rendering many visuals
concurrently (an entire effects library, each showing what it
would look like applied to the current stack). Impossible on
lpvm, easy on the GPU. That's its own scoped initiative.

**Design implication for this roadmap:** Keep the per-shader
backend trait surface in lpfx backend-agnostic so a wgpu impl
drops in alongside lpvm without restructuring the graph layer.

---

## D8: LFO and signal generators are Show-layer concerns,
not lpfx

**Context:** Initially proposed an LFO node in lpfx to exercise
the bus + bindings.

**Decision:** Punt LFO entirely. Signal generators (LFO, audio
analysis, MIDI/OSC, schedulers) belong to the Show layer that
lp-engine eventually adds around lpfx.

**Rationale:** Putting them in lpfx muddles the visual-
subsystem boundary. The bus is the seam; lpfx consumes the
bus, doesn't produce signals. Editor synthesises bus values
synthetically (sliders, manual values, a clock) — the same
contract a real Show would use.

---

## D9: Bus debugger is text-only, no plotting

**Context:** Considered plot history, multi-channel
visualization.

**Decision:** Text grid only — `(channel, kind, current value,
last writer, last write frame)`. Refreshes each frame.

**Rationale:** First-pass introspection to confirm the bus is
working. Plotting adds real UI surface; defer until needed.

---

## D10: M1 scope is Pattern alone, no bus

**Context:** Initial proposal was Pattern + bus + LFO in M1.

**Decision:** M1 is Pattern alone. Bus arrives in M5.

**Rationale:** "One thing at a time." Without LFO, the bus
isn't needed to exercise procedural signals (because there
*are* no procedural signals in lpfx). M1 reads param values
directly; the M5 bus integration swaps `params.get(name)` for
`bus.read(name)` — a localized refactor.

---

## D11: Seven milestones, M1+M2 parallelizable

**Context:** Initial 3-milestone user suggestion was expanded
to 5 by the assistant, then refined.

**Decision:**

| #  | Milestone               | Strategy        | Depends on  |
|----|-------------------------|-----------------|-------------|
| M1 | lpfx runtime            | C — full plan   | —           |
| M2 | lp-studio app core      | C — full plan   | —           |
| M3 | Pattern editor          | B — small plan  | M1 + M2     |
| M4 | Stack + Effect          | C — full plan   | M3          |
| M5 | Bus + bindings          | C — full plan   | M4          |
| M6 | Semantic editor         | C — full plan   | M5          |
| M7 | Cleanup + verification  | B — small plan  | M6          |

**Rationale:**

- M1's original "rewire + runtime + UI" was three concerns.
  Split: M1 = headless runtime, M2 = UI shell with no
  rendering, M3 = integration. Now each milestone has a
  single coherent acceptance signal.
- M1 and M2 parallelize because they touch independent code
  surfaces (lpfx vs lp-app/lp-studio); M3 is the join.
- Stack and Effect stay together (M4) — conceptually paired,
  example corpus uses them together.
- Bus is its own milestone (M5) — meaningful surface in both
  lpfx and lp-studio, plus default-bind behaviour and
  cascade rules.
- Semantic editor is its own milestone (M6) — TOML
  side-by-side, rich widgets, composition widgets, multi-tab,
  file tree CRUD. Largest editor-side milestone; deepest
  test of the Slot grammar.

---

## D12: lp-domain edits during this roadmap are in-bounds

**Context:** This roadmap exercises the lp-domain model end-
to-end and may surface shape issues (Slot grammar tweaks,
missing Presentation hints, Binding extensions).

**Decision:** lp-domain edits land directly in lp-domain
during this roadmap. The deferral of lp-domain M4–M6 (the
immutability gates) is precisely what makes that safe.

**Rationale:** The whole point of validating before locking is
that we can iterate freely. Edits to `examples/v1/` are also
free game.

---

## D13: TOML side-by-side editor in M6, not earlier

**Context:** Considered including TOML editor in M1/M3.

**Decision:** M6.

**Rationale:** M3's auto-generated widget panel covers the
"tweak params" case end-to-end. TOML editor adds a real
component (parse/serialize sync, error display) that belongs
with the rest of the semantic editor. For M1–M5 you can edit
TOML in a real editor and reopen to pick up changes.

---

## D14: Palettes and gradients are height-one texture resources

**Context:** Texture access has landed in `lp-shader`. GLSL shaders
now declare `sampler2D` uniforms and callers provide a strict
`TextureBindingSpec` per sampler, plus runtime `LpsTexture2DValue`
uniforms. The texture-access design explicitly keeps palette/gradient
baking above `lp-shader`.

**Decision:** In the lpfx MVP, palettes and gradients are authoring
recipes that materialize to width-by-one texture resources. Shaders
sample them as `sampler2D` uniforms compiled with
`texture_binding::height_one(...)`, not as fixed-size
`Kind::ColorPalette` / `Kind::Gradient` uniform structs.

**Rationale:** This keeps one shader-visible image mechanism for
Effect inputs, Stack inputs, bus textures, palettes, gradients, and
future images. It also matches the wgpu-shaped resource model:
lp-shader samples and validates; lpfx/domain decide source, routing,
baking, allocation, cache keys, invalidation, and lifetime.

**Design implication for this roadmap:** lpfx's backend trait and
runtime need texture binding specs and runtime texture uniforms from
M1. Stack ping-pong textures and generated resource textures must be
modeled as different lifetimes. lp-studio persists palette/gradient
recipes in TOML and rebakes runtime textures on edit; it does not
store baked texture bytes in localStorage.

---

## D15: Structured `params` uniform for authored shader parameters

**Context:** With nested texture fields now supported in `lp-shader`, we can put
texture-valued authored params like palettes and gradients inside a single
`params` struct alongside scalar params. Previously we considered this but
deferred because the language didn't support it.

**Decision:** 
- Shader-visible authored parameters are passed as a single `params` struct
  uniform that mirrors `ParamsTable`.
- Scalar params become fields like `params.speed`, `params.intensity`.
- Texture-valued params (palette, gradient) become fields like
  `params.palette`, `params.gradient` with dotted texture binding specs.
- Graph-fed texture inputs (Effect upstream, bus textures) remain outside
  authored `params` as resource uniforms; the naming convention for these
  is TBD in M4 Stack/Effect design.

**Rationale:** 
- `ParamsTable` is already an implicit `Shape::Struct`; the shader ABI should
  mirror the domain model.
- One struct reduces top-level uniform sprawl and makes param access consistent.
- Palette/gradient as texture fields inside `params` keeps them in the
  authored parameter surface rather than as separate top-level resources.

**Design implication:**
- lpfx helper derives `LpsType::Struct` for `Params`, builds
  `LpsValueF32::Struct` at runtime, and uses dotted paths like
  `params.gradient` for texture binding specs.
- Example shaders migrate from `param_speed` to `params.speed`.
- Graph inputs like `inputColor` are likely stale naming; M4 design should
  pick between `input`, `inputImage`, or `inputTex`.
