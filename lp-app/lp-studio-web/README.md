# lp-studio-web

`lp-studio-web` is the first static browser shell for LightPlayer Studio.

It renders `lp-studio-core` state and drives browser runtimes from
`lp-studio-runtime`: the browser-local `browser-worker` proof path and the
`browser-serial-esp32` path for already-flashed ESP32 hardware. It does not own
Studio domain behavior and does not use Dioxus server functions.

## Run

```bash
just studio-dev
```

`studio-dev` builds debug wasm artifacts for `lp-studio-web` and `fw-browser`,
packages them with wasm-bindgen, prepares the worker assets, and serves
`http://127.0.0.1:2820/`.

Use `just studio-web-build` or `just studio-web` when you want the release/static
build path.

## Hardware

The hardware button uses Web Serial, so it requires a supported Chromium-class
browser and a secure/local context. The current hardware path assumes the ESP32
already has LightPlayer firmware running; browser-side flashing is planned as
the next hardware phase.

The app loads `public/browser-serial.js` before the Rust wasm module. That shim
owns the direct Web Serial stream objects, while Rust owns Studio actions,
status, protocol parsing, and demo project upload.

## Stories

`lp-studio-web` has a native Dioxus storybook for isolated component states.
Stories live next to the components they exercise, using sibling files such as
`device_panel_stories.rs`.

Run the dev server and open the storybook:

```bash
just studio-dev
```

Then visit `http://127.0.0.1:2820/#/stories`.

Add new stories by:

1. adding a `*_stories.rs` sibling module for the component
2. adding one or more `StoryDescriptor` values
3. adding a `render_story` match arm for each stable story id
4. registering the module in `stories/story_registry.rs`

Use `stories/story_fixtures.rs` for fake but domain-shaped `StudioState`
fixtures. Stories should render real components, not duplicate mock markup.

Generate local PNGs for quick review:

```bash
just studio-story-pngs
```

PNGs are written to `lp-app/lp-studio-web/story-images/.scratch/`, which is
gitignored.

Update committed visual baselines when intentional Studio UI changes affect
component rendering:

```bash
just studio-story-baselines
```

Baselines are written to `lp-app/lp-studio-web/story-images/` and should be
committed when they change. The baseline set is intentionally small and should
stay curated. Hidden child directories under `story-images/` are scratch space
and are ignored. Story captures are clipped to the story canvas content at the
standard wide story viewport.

Baseline and check commands require `oxipng` so fresh captures compare against
the committed optimized PNGs.

Compare fresh story PNGs against the committed baselines without updating them:

```bash
just studio-story-check
```

Fresh check output is written to `lp-app/lp-studio-web/story-images/.new/`,
which is gitignored. For agent and pre-commit-style local flows, use:

```bash
just studio-story-baselines-if-needed
```

That command runs baseline generation only when tracked or untracked
non-generated files under `lp-app/lp-studio-web/` have changed.

## Boundary

- `lp-studio-core` owns actions, state, effects, diagnostics, and sessions.
- `lp-studio-runtime` owns browser worker/serial protocol flow and demo project
  loading.
- `lp-studio-web` owns Dioxus components and static presentation.
