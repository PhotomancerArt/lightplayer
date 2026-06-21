# lpa-studio-web

`lpa-studio-web` is the first static browser shell for LightPlayer Studio.

It renders `lpa-studio-core` state and drives browser runtimes from
`lpa-studio-runtime`: the browser-local `browser-worker` proof path and the
`browser-serial-esp32` path for already-flashed ESP32 hardware. It does not own
Studio domain behavior and does not use Dioxus server functions.

The main app uses a browser-side provisioning controller rather than helper
functions that return a completed `StudioApp`. The controller dispatches real
`StudioActionKind` values, executes returned effects through the active browser
runtime, applies events back into `StudioApp`, and auto-advances only obvious
steps such as endpoint granted -> connect -> read project state.

## Run

```bash
just studio-dev
```

`studio-dev` builds debug wasm artifacts for `lpa-studio-web` and `fw-browser`,
packages them with wasm-bindgen, prepares the worker assets, and serves
`http://127.0.0.1:2820/`.

Use `just studio-web-build` or `just studio-web` when you want the release/static
build path. The release build packages ESP32-C6 firmware assets for the future
browser flashing path.

## Hardware

The USB ESP32 provider uses Web Serial, so it requires a supported
Chromium-class browser and a secure/local context. The hardware path can connect
to an ESP32 that already has LightPlayer firmware running and can flash packaged
ESP32-C6 firmware when the firmware manifest is present.

The app loads `public/browser-serial.js` and `public/browser-esp32-flash.js`
before the Rust wasm module. The serial shim owns the direct Web Serial stream
objects for normal server protocol traffic. During firmware flashing, Studio
releases the normal serial reader/writer and the flashing shim takes exclusive
ownership of the same browser `SerialPort`.

The web controller auto-advances hardware provisioning in small explicit steps:
grant endpoint access, open the serial link, probe for a running LightPlayer
server, offer firmware flashing for provisionable ESP32-C6 bootloader targets,
reconnect after a successful flash, probe again, and finally read server
project state. Probe and reconnect failures are surfaced in the device manager
with recovery actions.

Release builds package firmware assets under:

```text
lp-app/lpa-studio-web/public/firmware/esp32c6/
```

The generated directory is gitignored. Regenerate it explicitly with:

```bash
just studio-firmware-package-esp32c6
```

The package contains `manifest.json` and a merged ESP32-C6 binary image produced
by `espflash save-image --merge --skip-padding`. The manifest records firmware
identity, build profile/features, source commit, flash address, size, checksum,
and reset/destructive-behavior notes. The browser flashing shim consumes this
manifest directly; it does not process ELF files or build firmware in the
browser.

## Stories

`lpa-studio-web` has a native Dioxus storybook for isolated component states.
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
Provisioning journey stories use `flow/*` ids and cover provider selection,
access, link opening, target probing, blank-device provisioning, flashing,
server ready, project-state reading, project selection, recovery, deploying,
ready, and connection-lost branches.

Generate local PNGs for quick review:

```bash
just studio-story-pngs
```

PNGs are written to `lp-app/lpa-studio-web/story-images/.scratch/`, which is
gitignored. Capture uses 4 Chrome pages by default; set
`STUDIO_STORY_PNGS_CONCURRENCY=<n>` to tune this locally.

Update committed visual baselines when intentional Studio UI changes affect
component rendering:

```bash
just studio-story-baselines
```

Baselines are written to `lp-app/lpa-studio-web/story-images/` and should be
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

Fresh check output is written to `lp-app/lpa-studio-web/story-images/.new/`,
which is gitignored. For agent and pre-commit-style local flows, use:

```bash
just studio-story-baselines-if-needed
```

That command runs baseline generation only when tracked or untracked
non-generated files under `lp-app/lpa-studio-web/` have changed.

## Boundary

- `lpa-studio-core` owns actions, state, effects, diagnostics, and sessions.
- `lpa-studio-runtime` owns browser worker/serial protocol flow and demo project
  loading.
- `lpa-studio-web` owns Dioxus components, static presentation, and the thin
  browser controller that routes core effects to browser runtimes.
