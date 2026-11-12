# M3 — Preview Web App (lpfx-demo)

A standalone app for developing and previewing effect modules. Shows
both CPU and GPU renderers side by side with auto-generated controls.

## Goal

A developer can open the app, see the `rainbow-noise.fx` effect
running live on both CPU (lpvm/WASM) and GPU (WebGPU), adjust inputs
via generated controls, and edit GLSL with hot-reload. No npm, no
node_modules — all Rust.

## Framework: Dioxus

Using Dioxus as a trial run for the main lp-app framework. Reasons:

- Cross-platform: web + desktop from one codebase. A native desktop
  version of the preview tool (and eventually lp-app) is valuable.
- React-like API (familiar component model).
- Hot reloading support.
- No npm dependency — builds with `dx` CLI or cargo.
- If it works well here, we use it for lp-app. If not, we learn that
  cheaply on a small app.

## Deliverables

### `lpfx/lpfx-demo` crate

A Dioxus app linking both `lpfx-cpu` and `lpfx-gpu`. Runs as a web
app (WASM) or native desktop app from the same source.

### UI layout

**Two canvases side by side:**
- Left: CPU (lpvm/WASM) rendering
- Right: GPU (WebGPU/wgpu) rendering
- Both driven by the same animation clock

**Auto-generated controls panel:**
- Built from the `FxManifest` input definitions
- For each input, a Dioxus component for the appropriate widget:
  - `f32` → slider + numeric display
  - `i32` with `choice` → dropdown/select
  - `bool` → toggle/checkbox
  - `Color` → color picker
- Controls update both CPU and GPU instances simultaneously
- Labels, ranges, units from the manifest

**GLSL editor pane:**
- Text area or embedded editor component
- Debounced recompile on edit (reloads both instances)
- Error display

**Status bar:**
- FPS for each renderer
- Compile time / status

### Build system

```just
demo-web:
    cd lpfx/lpfx-demo && dx serve --platform web

demo-desktop:
    cd lpfx/lpfx-demo && dx serve --platform desktop

demo-build:
    cd lpfx/lpfx-demo && dx build --platform web --release
```

No npm, no node_modules. Just `dx` (Dioxus CLI) + cargo.

### Embedded effects

The demo ships with `rainbow-noise.fx` embedded (inlined as const
strings via `include_str!`). Future: file picker or drag-and-drop to
load custom `.fx` directories.

## Dependencies

- M1 (lpfx-cpu, CPU rendering works)
- M2 (lpfx-gpu, GPU rendering works)

## Validation

- Open in browser, see two animated canvases
- Adjust speed slider → both renderers respond
- Change noise function dropdown → both switch
- Edit GLSL → both recompile and update
- CPU and GPU outputs are visually similar (not identical)
