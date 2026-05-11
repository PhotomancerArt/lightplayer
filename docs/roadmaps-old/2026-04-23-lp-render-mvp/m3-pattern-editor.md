# Milestone 3: Pattern editor — wire lpfx runtime to lp-studio, end-to-end

## Goal

Bring M1 (lpfx runtime, headless) and M2 (lp-studio app core, no
rendering) together. Add wasm interop between Dioxus and the lpfx
runtime, an HTML canvas preview pane, an "open pattern" UI flow
that loads from the virtual fs, and an auto-generated widget
panel that maps `Pattern.params` to scalar widgets. Result: open
`rainbow.pattern.toml`, see it render live in the preview, drag
the speed slider, watch the rainbow accelerate.

M3 also proves the new texture-backed palette/gradient path in the
editor: a Pattern can declare a `sampler2D` palette/gradient strip,
lpfx bakes the authoring value into a width-by-one texture, and the
preview re-renders when the editor changes the recipe.

This is the **first end-to-end edit-a-pattern experience** and the
first integration test of the lp-domain model under load.

## Suggested plan location

`docs/roadmaps/2026-04-23-lp-render-mvp/m3-pattern-editor/`

Small plan: a single `plan.md` covering the integration phases.

## Scope

**In scope:**

- **Wasm interop bridge** in
  `lp-app/lp-studio/src/runtime_bridge.rs`:
  - lpfx is `no_std + alloc`; lpfx-cpu compiles fine in wasm
    (proven by lp-app/web-demo today via lpvm-wasm).
  - lp-studio depends on lpfx + lpfx-cpu; instantiates an
    `Engine<LpvmShaderBackend>` once at app boot.
  - Engine instance lives in a Dioxus context provider so
    components access it via hook.
- **Canvas preview pane** in
  `lp-app/lp-studio/src/components/preview.rs`:
  - Dioxus component wrapping an `HTMLCanvasElement`.
  - Render loop: `requestAnimationFrame` calls into a `render`
    closure that runs `PatternInstance::render` and blits the
    `LpsTextureBuf` to the canvas via `ImageData` /
    `putImageData` (or a small WebGL2 texture upload — design
    phase decides).
  - Resolution selector (256×256 default, configurable in
    UI later).
  - Pause / play toggle, frame counter for debugging.
- **Pattern editor page** in
  `lp-app/lp-studio/src/pages/pattern_editor.rs`:
  - Layout: file picker on the left (or top), preview pane on
    the right, params panel below preview.
  - File picker lists all `*.pattern.toml` files in the
    virtual fs.
  - Selecting a file: load via `lpfx::load_pattern(&fs, path)`,
    instantiate via `Engine::instantiate_pattern`, swap the
    preview's render closure.
  - Param panel: walks `Pattern.params.0` (the root `Slot`), builds
    `params` struct values for the shader; calls `lp_studio_widgets::widget_for_slot(slot)`
    for each, `on_change` handlers update the in-memory param values
    that `PatternInstance` reads each frame.
  - Texture-backed palette/gradient params: the widget writes the
    authoring recipe, lpfx rebakes the corresponding height-one
    resource texture and binds it as `params.gradient` (or `params.palette`),
    and the render loop binds it as a `sampler2D` uniform on the next frame.
  - Edits to params **don't persist to fs** in M3 — they're
    in-memory tweaks. Saving back to TOML is M6's
    semantic-editor concern. Reset = pick the file again.
- **Routing**: `/pattern/:path` route in lp-studio's router
  opens a specific pattern file directly. From the M2 files
  page, "open in editor" link routes to the pattern editor.
- **Smoke tests** in `lp-app/lp-studio/tests/` (or via wasm-bindgen
  tests, design phase decides):
  - `wasm-pack test` confirms the runtime bridge instantiates a
    Pattern and produces non-zero output.
  - Runtime bridge test for a texture-backed Pattern confirms a
    height-one palette/gradient texture is baked, bound, and sampled
    through `LpsTextureBuf::to_named_texture_uniform`.
  - Manual acceptance: open rainbow → see rolling rainbow,
    drag speed → rainbow speeds up, switch to fbm → see fbm,
    refresh page → editor restores last-opened file from
    localStorage (the file content survives via M2's fs;
    "last opened" is a UI concern that may or may not land
    here).

**Out of scope:**

- Stack / Effect editor (M4).
- Bus + bindings + binding UI (M5).
- TOML side-by-side editor (M6).
- Saving param edits back to fs (M6).
- File creation / deletion / renaming UI (M6 file tree
  refinements).
- Performance work — naive blit to canvas is fine for M3;
  optimization happens when needed.
- Hot reload of edited GLSL (later).

## Key decisions

- **Engine instance is a singleton via Dioxus context.** One
  lpfx `Engine` per app, lives for the app's lifetime. Avoids
  re-compiling shaders on every component mount.
- **Render loop drives `requestAnimationFrame`, not Dioxus's
  re-render cycle.** Dioxus re-renders on state changes; the
  preview canvas must re-render every frame even when no state
  changed. Decoupling them means param edits update the in-memory
  state, and the rAF loop reads that state on the next tick.
- **Param edits are in-memory in M3, not persisted.** The
  in-memory `PatternInstance` holds a mutable `BTreeMap<String,
  LpsValue>` (or similar) that the widget panel writes into.
  Persisting back to TOML is a serialization roundtrip that
  belongs in M6's semantic editor.
- **Palette/gradient edits are recipe edits, not binary asset
    edits.** M3 may rebake runtime texture bytes repeatedly, but the
    editor state remains the authoring recipe. No baked texture bytes
    are persisted in localStorage.
- **`params` is the shader-visible parameter surface.** The editor
    panel builds `params` struct values that the shader reads via a
    single `Params` uniform, not flat top-level uniforms.
- **Texture-valued params use dotted paths like `params.gradient`.**
    Palette and gradient resources are bound as shader fields inside
    the `params` struct using canonical dotted texture spec keys.
- **Canvas blit strategy** is a design-phase decision. Two
  candidates: `putImageData` (simple, software-side, slower) and
  a tiny WebGL2 program that uploads `LpsTextureBuf` as a
  texture and draws a fullscreen quad (faster but more setup).
  M3 picks one based on what's good enough for 256×256@60fps.
- **Render at fixed resolution** (256×256 or so) in M3.
  Resolution control is a later refinement.
- **Hot reload** (file change → re-instantiate Pattern) is
  deferred. M3 requires reopening the file to pick up changes.

## Deliverables

- `lp-app/lp-studio/src/runtime_bridge.rs` (Engine context).
- `lp-app/lp-studio/src/components/preview.rs` (canvas component
  + render loop).
- `lp-app/lp-studio/src/pages/pattern_editor.rs`.
- `widget_for_slot(&Slot) -> Element` in `lp-studio-widgets`
  hooked up to scalar `Pattern.params` slots.
- Routing: `/pattern/:path`.
- Wasm interop tests (or smoke harness).
- Texture-backed Pattern smoke covering height-one resource
  allocation, rebake, binding, and sampling.
- lp-studio README updated with the new editor page.

## Acceptance smoke tests

```bash
cargo build -p lp-studio
dx build --release  # in lp-studio/

# Wasm-side tests (if used):
wasm-pack test --headless --chrome lp-app/lp-studio

# Manual acceptance:
cd lp-app/lp-studio && dx serve
# → open /files, click rainbow.pattern.toml → "open in editor"
# → preview shows rolling rainbow
# → speed slider visible, draggable, rainbow speeds up
# → switch to fbm.pattern.toml → preview switches
# → open texture-backed gradient/palette test pattern
# → adjust the palette/gradient widget and see the preview update
# → refresh → fs state persists (file content unchanged)
```

## Dependencies

- M1 complete (lpfx runtime + Pattern node + loader/cache).
- M2 complete (lp-studio shell + virtual fs + scalar widgets).

## Execution strategy

**Option B — Small plan (`/plan-small`).**

Justification: Pure integration. Architectural calls were made
in M1 (runtime shape, backend trait) and M2 (widget API, fs
layer). Real design surface here is canvas integration (blit
strategy) and Dioxus rAF coordination — tactical, not
architectural. Two-three phases at most: bridge + engine
context, preview canvas + rAF loop, pattern editor page +
widget panel + routing.

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?
