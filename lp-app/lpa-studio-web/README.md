# lpa-studio-web

`lpa-studio-web` is the static browser shell for the `lpa-studio-ux` slice.

The web app owns Dioxus presentation. It renders `StudioView` panes, renders
contextual `UxAction` controls, and dispatches those actions back into
`StudioUx`. Browser-worker lifecycle, provider routing, protocol request
correlation, running-project attach, demo project deployment, and project
inventory reads belong below the UI in `lpa-studio-ux`, `lpa-link`, and
`lpa-client`.

## Current Slice

The active first screen is the Link pane, rendered from provider actions owned
by the UX layer. In the browser build it starts with simulator and ESP32
actions:

```text
lpa-studio-web -> lpa-studio-ux -> LinkProviderRegistry -> browser-worker -> fw-browser -> lp-server
```

`LinkUx` owns `LinkProviderRegistry`, turns provider choices into contextual
`UxAction` values backed by `LinkOp` operations, and opens link sessions through
the selected provider. The simulator provider auto-discovers and connects its
single browser-worker endpoint, so starting the simulator is one click.
`ServerUx` owns the `lpa-client` protocol client once a connected link exposes
server I/O.

The WebSerial ESP32 provider is visible as a provider action when browser serial
support is compiled in. The browser still owns the serial port picker and
permission prompt; the UI does not model that picker as an endpoint-selection
screen.

The slice can launch the browser-local firmware runtime, connect browser serial
hardware, open the server protocol, attach to an already-loaded running project,
load the built-in demo project, and display a small project inventory summary.
It intentionally does not include the previous full ESP32 provisioning,
flashing, and recovery UI.

## Run

```bash
just studio-dev
```

`studio-dev` builds debug wasm artifacts for `lpa-studio-web` and `fw-browser`,
packages them with wasm-bindgen, prepares the wasm sidecar assets, and serves
`http://127.0.0.1:2820/`.

Use `just studio-web-build` or `just studio-web` for the release/static build
path. The release build still packages ESP32-C6 firmware assets for future
browser flashing work.

Browser-worker assets are served from `public/pkg/`. The UX boot path resolves
those paths to page-absolute URLs before sending them into the embedded blob
worker, which lets worker import/init failures surface as actionable link
errors instead of silent boot timeouts.

## Stories

The storybook has been reduced to the new UX shell, action, and pane states for
this experiment.
Run the dev server and open:

```text
http://127.0.0.1:2820/#/stories
```

Generate or update visual baselines with:

```bash
just studio-story-baselines-if-needed
```

The baseline set intentionally reflects the active UX simulator surface rather
than the old provisioning journey fixtures.

## Boundary

- `lpa-studio-ux` owns Studio product state, `StudioView` panes, snapshots,
  actions, async dispatch, UX node ids, the link provider registry, and the
  connected server client.
- `lpa-link` owns provider implementations, provider resources, sessions, and
  lifecycle.
- `lpa-client` owns server protocol correlation and typed project operations.
- `lpa-studio-web` owns Dioxus rendering, pane composition, and browser event
  handling.
