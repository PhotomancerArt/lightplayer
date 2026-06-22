# lpa-studio-web

`lpa-studio-web` is the static browser shell for the `lpa-studio-ux` slice.

The web app owns Dioxus presentation. It renders `StudioView` panes and
contextual `UxAction` controls, then dispatches those actions back into
`StudioUx`. It also applies live `UxUpdate` values while long async actions are
running. Browser-worker lifecycle, provider routing, protocol request
correlation, running-project attach, demo project deployment, and project
inventory reads belong below the UI in `lpa-studio-ux`, `lpa-link`, and
`lpa-client`.

## Current Surface

The active first screen is the Link pane, rendered from provider actions owned
by the UX layer. In the browser build it starts with simulator and ESP32
actions:

```text
lpa-studio-web -> lpa-studio-ux -> LinkProviderRegistry -> browser-worker -> fw-browser -> lp-server
```

`LinkUx` owns `LinkProviderRegistry`, turns provider choices into contextual
`UxAction` values backed by `LinkOp` operations, and opens link sessions through
the selected provider. The simulator provider auto-discovers and connects its
single browser-worker endpoint, opens the server protocol, and auto-loads the
demo project when no project is already running. Starting the simulator is one
click.
`ServerUx` owns the `lpa-client` protocol client once a connected link exposes
server I/O.

The WebSerial ESP32 provider is visible as a provider action when browser serial
support is compiled in. The browser still owns the serial port picker and
permission prompt; the UI does not model that picker as an endpoint-selection
screen.

The current surface can launch the browser-local firmware runtime with the demo
project, connect browser serial hardware, open the server protocol, attach to an
already-loaded running project, explicitly load the built-in demo project on
hardware, provision a blank ESP32-C6 with packaged LightPlayer firmware, reset a
provisioned ESP32-C6 back to blank, and display a small project inventory
summary.

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

ESP32-C6 firmware assets are served from
`public/firmware/esp32c6/manifest.json`. Browser serial provisioning imports a
pinned browser ESM `esptool-js` module from
`https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm` by default; deployments can
override the `BrowserSerialEsp32Options` path if they want to serve that module
themselves. The CDN ESM endpoint avoids raw package bare imports such as `pako`,
which browsers cannot resolve directly, and it decodes the ESP32-C6 flasher
stub used by reset/provisioning. Firmware provisioning and reset-to-blank both
require a browser with Web Serial support and a user-granted serial port.

## Hardware Flow

Start the dev server, open `http://127.0.0.1:2820/`, and choose the ESP32 Web
Serial action. Browser port selection is handled by the browser permission
prompt, not by a Studio endpoint picker.

For a blank or non-LightPlayer ESP32-C6, Studio keeps the link session and
offers `Provision firmware`. Confirming the action writes the packaged firmware
and then attempts to reconnect to the LightPlayer server after reset. Flashing
renders live progress and raw esptool output in the Link pane.

For an already provisioned ESP32-C6, Studio can connect to the server/project
workflow. The link pane also offers `Reset to blank` as a destructive tertiary
action when the provider advertises whole-device erase. Confirming it erases the
device flash, clears server/project state, and returns the link to a
provisionable state. Reset-to-blank uses the same live activity renderer.

## Stories

The storybook covers the active UX shell, action strip, pane states,
provision-ready/provisioning/provision-failed, and reset-to-blank states.
Run the dev server and open:

```text
http://127.0.0.1:2820/#/stories
```

Generate or update visual baselines with:

```bash
just studio-story-baselines-if-needed
```

The baseline set intentionally reflects the active view-driven UX surface rather
than the old provisioning journey fixtures.

## Boundary

- `lpa-studio-ux` owns Studio product state, `StudioView` panes, snapshots,
  actions, live `UxUpdate` activity, async dispatch, UX node ids, the link
  provider registry, and the connected server client.
- `lpa-link` owns provider implementations, provider resources, sessions, and
  lifecycle.
- `lpa-client` owns server protocol correlation and typed project operations.
- `lpa-studio-web` owns Dioxus rendering, view composition, and browser event
  handling.
