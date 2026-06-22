# lpa-studio-web

`lpa-studio-web` is the static browser shell for the experimental
`lpa-studio-ux` slice.

The web app owns Dioxus presentation. It renders `StudioSnapshot` values,
renders contextual `AvailableAction<StudioAction>` controls, and dispatches
actions back into `StudioUx`. Browser-worker lifecycle, provider routing,
protocol request correlation, demo project deployment, and project inventory
reads belong below the UI in `lpa-studio-ux`, `lpa-link`, and `lpa-client`.

## Current Slice

The active first screen is the browser simulator, reached through the
provider/endpoint model owned by `lpa-link`:

```text
lpa-studio-web -> lpa-studio-ux -> LinkProviderRegistry -> browser-worker -> fw-browser -> lp-server
```

`LinkUx` owns `LinkProviderRegistry`, renders provider and endpoint choices,
and opens link sessions through the selected provider. `ServerUx` owns the
`lpa-client` protocol client once a connected link exposes server I/O. The slice
can launch the browser-local firmware runtime, open the server protocol, load
the built-in demo project, and display a small project inventory summary. It
intentionally does not include the previous Web Serial ESP32 provisioning UI or
the full old component set.

The older `lpa-studio-core` and `lpa-studio-runtime` crates remain in the
workspace as references during the experiment, but the default web app does not
depend on them.

## Run

```bash
just studio-dev
```

`studio-dev` builds debug wasm artifacts for `lpa-studio-web` and `fw-browser`,
packages them with wasm-bindgen, prepares the wasm sidecar assets, and serves
`http://127.0.0.1:2820/`.

Use `just studio-web-build` or `just studio-web` for the release/static build
path. The release build still packages ESP32-C6 firmware assets for future
browser flashing work, even though the current UX slice is simulator-only.

## Stories

The storybook has been reduced to the new UX shell states for this experiment.
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

- `lpa-studio-ux` owns Studio product state, snapshots, actions, async
  execution, the link provider registry, and the connected server client.
- `lpa-link` owns provider implementations, provider resources, sessions, and
  lifecycle.
- `lpa-client` owns server protocol correlation and typed project operations.
- `lpa-studio-web` owns Dioxus rendering and browser event handling.
