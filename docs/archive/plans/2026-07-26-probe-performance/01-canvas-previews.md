# P1 — Canvas-based preview rendering

Size: sm. Depends on: nothing. Parallel-safe with P2/P3.

## Scope

Replace the DOM-grid rendering of visual product previews with a `<canvas>`
painted via `putImageData`. One rendered preview = one DOM element, regardless
of probe resolution.

Out of scope: probe request size/format changes (P4), which nodes are probed
(P5), the PreviewHost/gallery path (already canvas-based).

## Current state

- `lp-app/lpa-studio-web/src/app/node/produced_product_view.rs:203-225` —
  `ProductPixelGrid(width, height, bytes: Rc<[u8]>)` renders a CSS grid of
  `width × height` keyed `<span>`s.
- `:364-374` — `rgb_pixel_styles` allocates one `format!("background-color:
  rgb(..)")` `String` per pixel. At 32×32 that is 1024 Strings + 1024 DOM
  nodes re-diffed per view snapshot (up to 30 Hz on sim).
- Preview bytes are tightly-packed RGB8 (`UiProductPreview::VisualSrgb8
  { width, height, revision, bytes }`, see
  `lp-app/lpa-studio-core/src/app/node/ui_produced_product.rs`).
- Canvas precedent in-repo: the gallery/preview-lab path paints RGBA8 frames
  to canvas (`lp-app/lpa-studio-web/src/app/home/gallery_preview.rs`) — reuse
  its pattern for getting a `web_sys::HtmlCanvasElement` from a Dioxus
  `onmounted` event and painting with
  `CanvasRenderingContext2d::put_image_data`.

## Implementation sketch

1. Add a `ProductPreviewCanvas` component (same props as `ProductPixelGrid`
   plus the preview `revision`):
   - `canvas` element with `width`/`height` attributes = probe resolution;
     CSS scales it to the card slot (keep existing container classes /
     aspect handling; `image-rendering: pixelated` to preserve the current
     crisp-pixel look).
   - On mount and whenever `(revision, bytes)` change, expand RGB8 →
     RGBA8 into a scratch `Vec<u8>` (alpha 255), build `ImageData` via
     `ImageData::new_with_u8_clamped_array_and_sh`, `put_image_data(0, 0)`.
   - Use `use_effect` keyed on the revision so paints happen only when new
     probe bytes actually arrived, not on unrelated re-renders.
2. Replace `ProductPixelGrid` call sites with the new component; delete
   `ProductPixelGrid` and `rgb_pixel_styles`.
3. Check other users: `rg "ProductPixelGrid|rgb_pixel_styles" lp-app` —
   stories/fixtures may reference them.

## Stories / baselines

Node-card stories that render previews will change appearance (spans →
canvas). Canvas paints from `use_effect` may need a settle before capture —
the capture pipeline already waits for `fonts.ready` + settle (see
docs/debt/story-capture-pipeline.md). If canvas painting proves flaky in
capture, prefer deterministic paint on mount over capture-side sleeps.
CI auto-commits baseline drift; expect a baseline-refresh commit.

## Conventions

- Match surrounding Dioxus idiom in `produced_product_view.rs` (component
  fn + `rsx!`), Tailwind class conventions (`tw:` prefix).
- No new deps; `web-sys` features may need additions in the crate's
  `Cargo.toml` (`ImageData`, `CanvasRenderingContext2d`,
  `HtmlCanvasElement`) — check what's already enabled.

## Validation

- `cargo check -p lpa-studio-web` (or `just check`).
- Story capture of a node card with a preview renders non-black canvas
  pixels (existing story tooling).
- `just check` green at phase end.

## Agent reminders

Do not commit unless asked. Do not expand scope. Do not suppress warnings or
disable tests. Stop and report if blocked. Report changes, validation, and
deviations.

ADR: none. Review gate: none (sim feel check batched at PR review).

## Definition of done

`ProductPixelGrid`/`rgb_pixel_styles` gone; previews render via canvas with
paints keyed on preview revision; checks green.

## Implementation Result

Status: done
Completed: 2026-07-27
Commit: e287c3d5d

- Changed: `ProductPreviewCanvas` + `paint_preview_canvas` in
  `produced_product_view.rs` (grid + `rgb_pixel_styles` deleted); CSS class
  renamed to `.ux-produced-product-pixel-canvas` with
  `image-rendering: pixelated`. Repaints keyed on `(revision, buffer
  identity)`; `onmounted` paint covers the first frame.
- Validated: `just check` + `just test` green; story baselines left to CI
  auto-commit (expected drift).
- Deviations: none. Details in [handoff.md](handoff.md).
