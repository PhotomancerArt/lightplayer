# lpa-studio-web

`lpa-studio-web` is the static browser shell for `lpa-studio-core`.

The web app owns Dioxus presentation. It renders `StudioView` panes and
contextual `UiAction` controls, then dispatches those actions back into
`StudioUx`. It also applies live `UxUpdate` values while long async actions are
running. Browser-worker lifecycle, provider routing, protocol request
correlation, running-project attach, demo project deployment, and project
inventory reads belong below the UI in `lpa-studio-core`, `lpa-link`, and
`lpa-client`.

## Current Surface

The active first screen is the Device pane, rendered from stack sections and
actions owned by the core layer. In the browser build it starts with simulator
and ESP32 connection actions:

```text
lpa-studio-web -> lpa-studio-core -> DeviceUx -> LinkProviderRegistry -> browser-worker -> fw-browser -> lp-server
```

`DeviceUx` is the user-facing workflow for selecting a connection, opening the
device session, attaching the LightPlayer server protocol, and handing off to
project controls. It owns the lower-level `LinkUx` and `ServerUx` internals, so
the web UI does not present separate Link and Server panes. The simulator
provider auto-discovers and connects its single browser-worker endpoint, opens
the server protocol, and auto-loads the demo project when no project is already
running. Starting the simulator is one click.

The WebSerial ESP32 provider is visible as a provider action when browser serial
support is compiled in. The browser still owns the serial port picker and
permission prompt; the UI does not model that picker as an endpoint-selection
screen.

The current surface can launch the browser-local firmware runtime with the demo
project, connect browser serial hardware, open the LightPlayer server protocol,
attach to an already-loaded running project, explicitly load the built-in demo
project on hardware, provision a blank ESP32-C6 with packaged LightPlayer
firmware, reset a provisioned ESP32-C6 back to blank, and render a readonly
project workspace once a project is loaded. Project attach/load choices appear
in the Device pane. The Project pane appears once a project is loaded.

## Run

```bash
just studio-dev
```

`studio-dev` builds debug wasm artifacts for `lpa-studio-web` and `fw-browser`,
packages `fw-browser` with wasm-bindgen, packages the ESP32-C6 firmware assets
used by browser flashing, mirrors those generated assets into Dioxus' dev
public directory, and serves `http://127.0.0.1:2820/` through `dx serve`.

Use `just studio-web-build` or `just studio-web` for the release/static build
path. `dx build` writes Studio app assets under
`target/dx/lpa-studio-web/{debug,release}/web/public/`, while `public/`
contains only hand-authored static files that are copied into that output.
Generated runtime sidecars are built under `target/studio-web-assets/` and then
mirrored into the generated Dioxus public directory. Release app assets are
hash-named under `assets/`. The release build still packages ESP32-C6 firmware
assets for future browser flashing work.

## Deploy

Production Studio deploys to `https://lightplayer.app/` through GitHub Pages
from Actions. Build a clean release artifact locally with:

```bash
just studio-web-deploy-dir production target/pages/studio lightplayer.app
just studio-web-smoke target/pages/studio
```

The deploy artifact is staged under `target/pages/studio` and includes
`version.json`, `changelog.json`, `.nojekyll`, and `CNAME`. It is built from the
release `dx` output so stale debug artifacts left by `studio-dev` are not
uploaded.

### Version badge

The header shows a build-info badge (an `Info` popover next to the title). It
fetches two static JSON files from the site root at runtime, so it always
reflects the *deployed artifact* rather than a compile-time constant:

- `version.json` — written by `scripts/pages/prepare-pages-artifact.mjs` on
  every Pages deploy. The popover shows version, channel, short commit sha
  (with a `dirty` marker when set), and build time.
- `changelog.json` — also written by that script, from git version tags
  (`vYYYY.MM.DD-N`, most recent 8). Each tag becomes one "Recent updates" line:
  a GitHub merge commit contributes the PR number and its body (the human PR
  title); any other tagged commit contributes its subject. Building it needs
  tags/history, so the Pages workflows check out with `fetch-depth: 0`; on a
  shallow or tagless tree it emits `entries: []` and the section is hidden.

Neither file is emitted by local dev builds (`dx serve`, `just studio-web-build`),
so the fetch 404s and the badge degrades gracefully to a "dev build" state with
the popover explaining that version metadata is only present in deployed builds.

Manual beta deployment uses the same artifact recipe with
`beta.lightplayer.app` and is published by the `Deploy Pages Channel` workflow.
Operational setup, DNS records, and GitHub Pages HTTPS steps are documented in
[`docs/deploy/studio-pages.md`](../../docs/deploy/studio-pages.md).

Browser-worker assets are served from `pkg/` in the generated site. The source
sidecar files are generated under `target/studio-web-assets/{debug,release}/pkg/`
and copied into `target/dx/lpa-studio-web/.../public/pkg/` after `dx` builds
the Studio app. The app-core boot path resolves those paths to page-absolute URLs
before sending them into the embedded blob worker, which lets worker import/init
failures surface as actionable link errors instead of silent boot timeouts.

Browser ESP32 Web Serial uses the shared app-served controller at
`public/lpa-link/browser_esp32_device_controller.js`. Both Studio's wasm-bound
`lpa-link` provider and the standalone `serial-debug.html` page import that
module, so normal connect/reset/read debugging exercises the same Web Serial
lifecycle code that Studio uses.

ESP32-C6 firmware assets are generated under
`target/studio-web-assets/firmware/esp32c6/` and served from
`firmware/esp32c6/manifest.json` in the generated site. Browser serial
provisioning imports a pinned browser ESM `esptool-js` module from
`https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm` by default; deployments can
override the `BrowserSerialEsp32Options` path if they want to serve that module
themselves. The jsDelivr ESM transform avoids raw package bare imports such as
`pako`, which browsers cannot resolve directly, and exposes the ESP32-C6
flasher stub JSON with the named exports expected by `esptool-js`. Firmware
flashing and device wipe both require a browser with Web Serial support and a
user-granted serial port.

## Hardware Flow

Start the dev server, open `http://127.0.0.1:2820/`, and choose the ESP32 Web
Serial action. Browser port selection is handled by the browser permission
prompt, not by a Studio endpoint picker.

For a blank or non-LightPlayer ESP32-C6, Studio keeps the device session and
offers `Flash firmware` in the LightPlayer step. Confirming the action
writes the packaged firmware and then attempts to reconnect to the LightPlayer
server after reset. Flashing renders live progress in the active Device step
and raw esptool output in the Console below the Device panel.

The Console panel (`app/device/runtime_log.rs`) renders the filtered
`UiConsoleView` from core. Its compact toolbar carries a funnel-marked
threshold select (`Level+`, default Info+ — the **display filter**), a
"Sources" popover of per-origin checkboxes with a hidden-source badge, a gear
popover holding the **device log level** select (what the connected device
emits, distinct from the display filter; disabled while disconnected), and
Clear; a right-aligned "N hidden" sliver appears only when the filter hides
entries. Rows are **container-responsive** (`app/../core/log_list.rs`): the
list is a CSS `@container`, so below 560px of its own width rows are two-line
(a dim `time · level · source` meta line over a full-width message, warn/error
marked by a left accent bar) and at 560px+ the same DOM relayouts into the
four-column time/level/source/message grid. Timestamps are UTC `HH:MM:SS`;
rendering caps at a 250-row tail of the filtered entries while the core ring
retains 1000.

During the initial browser-serial server attach, the Device pane shows a
stepped readiness activity while raw boot lines stream into the Console below
the Device panel. Blank or erased devices are recognized from ESP32 ROM output
such as `invalid header: 0xffffffff`, so the app lands in a provision-ready
state instead of a generic action failure.

For an already provisioned ESP32-C6, Studio can connect to the server/project
workflow. The Device pane also offers `Wipe device` as a destructive
tertiary action when the provider advertises whole-device erase. Confirming it
erases the device flash, clears server/project state, and returns the device to a
provisionable state. Wipe uses the same live activity renderer.

Project refresh is passive background work in the web shell. Device recovery
actions such as disconnect, reset, flash, and wipe preempt passive refresh so
older firmware or a stuck project read cannot trap the user away from firmware
recovery controls.

For low-level browser serial debugging, open:

```text
http://127.0.0.1:2820/serial-debug.html
```

The page can select a Web Serial port, run the same normal reset/read path as
Studio, exercise explicit USB-JTAG downloader reset experiments, and show raw
serial output without involving the full Studio UX.

## Theme And Layout

Studio web styling is Tailwind-first. Components should prefer semantic
Tailwind utilities in their Dioxus markup, using the existing `tw:` prefix while
legacy `ux-*` classes still exist. Theme values are defined as Studio CSS
variables in `src/style.css` and exposed to Tailwind from `tailwind.css` with
semantic names such as `background`, `card`, `border`, `muted-foreground`,
`accent`, and `status-warning-bg`.

Use direct utility strings for simple static styling. Use small Rust helper
functions for repeated stateful variants such as status tones, action priority,
step state, pane emphasis, and project node status. Avoid adding broad new
selector families to `src/style.css`; that file should stay limited to theme
variables, base rules, keyframes, browser/measurement behavior, and explicitly
transitional story or exploration surfaces.

Reusable Dioxus surfaces live under `src/base`, `src/core`, and `src/app`:

- `ActionButton` and `ActionStrip` render `UiAction` controls.
- `PaneFrame`, `StatusChip`, and `MetricGrid` provide shared pane structure.
- `ProjectSidebar` renders the Project rail with compact node tree, project
  stats, and project actions.
- `ProjectNodeWorkspace` renders all synced node bodies in tree order as the
  transparent center workspace.
- `FieldRow` and `Tabs` remain editor-foundation primitives used by stories and
  future editing surfaces.
- `StudioShell`, `UxPane`, and `RuntimeLog` render the active `StudioView`.

The project editor layout target is:

```text
lg: [ node tree ] [ nodes/editor ] [ device/secondary ]
md: [ nodes/editor ] [ tabs: node tree / device / bus / console ]
sm: [ tabs: nodes / node tree / device / bus / console ]
```

The active Project pane currently renders readonly synced node data. Slot
editing, overlay dirty-state, binding authoring, bus modeling, probes, and
asset editing belong to later milestones.

## Stories

The storybook covers the active Studio shell, connection action strip, Device stack
states, loaded Project pane state with readonly node workspace,
browser-serial blank-firmware readiness, provision-ready/provisioning/
provision-failed, wipe states, the version badge (loaded + dev-build fallback),
and editor-foundation primitives.
Run the dev server and open:

```text
http://127.0.0.1:2820/#/stories
```

Generate or update visual baselines with:

```bash
just studio-story-baselines-if-needed
```

Baselines are captured for `sm`, `md`, and `lg` viewports. Files are named as a
story id plus viewport suffix, for example:

```text
studio__editor-shell__sm.png
studio__editor-shell__md.png
studio__editor-shell__lg.png
```

Useful commands:

```bash
just studio-story-pngs        # scratch captures under story-images/.scratch
just studio-story-baselines   # update committed sm/md/lg baselines
just studio-story-check       # compare fresh captures with committed baselines
```

Baseline and check modes require `oxipng` so committed and fresh PNGs are
losslessly normalized. Install it with `brew install oxipng` or
`cargo install oxipng`. The capture script defaults to one Chrome page for
stable baseline/check output; set `STUDIO_STORY_PNGS_CONCURRENCY` for faster
scratch runs when needed.

Captures disable CSS transitions and animations before the app mounts so
every screenshot shows the settled end state; without this, captures raced
150ms transitions and landed at a different phase each run. Check mode also
compares pixels with a small tolerance for residual jitter (anti-aliasing and
sub-pixel text layout, which move a handful of glyph-edge pixels between
captures of the same build). A pixel counts as significantly different when
its per-channel delta exceeds `STUDIO_STORY_MAX_CHANNEL_DELTA` (default `64`,
above anti-aliasing noise); an image fails only when the fraction of such
pixels exceeds `STUDIO_STORY_MAX_DIFF_PIXEL_RATIO` (default `0.0005`, i.e.
0.05%). This gives the check a small noise floor — changes below the ratio
don't fail it, but they still show up as a baseline image diff in the PR.
Dimension changes and undecodable PNGs always fail. Images that differ in
bytes but stay within tolerance are listed informationally, and the summary
line reports how many baselines were byte-identical.

The baseline set intentionally reflects the active view-driven UX surface,
including the semantic project workspace, rather than the old provisioning
journey fixtures alone.

## Boundary

- `lpa-studio-core` owns Studio product state, `StudioView` panes, stack views,
  snapshots, actions, live `UxUpdate` activity, async dispatch, UX node ids, the
  link provider registry, and the connected server client.
- `lpa-link` owns provider implementations, provider resources, sessions, and
  lifecycle.
- `lpa-client` owns server protocol correlation and typed project operations.
- `lpa-studio-web` owns Dioxus rendering, view composition, and browser event
  handling.
