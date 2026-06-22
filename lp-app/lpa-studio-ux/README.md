# lpa-studio-ux

`lpa-studio-ux` is the UI-independent Studio UX layer.

`Ux` means a resource-owning product surface. The crate owns lower-level
services such as `lpa-link` and `lpa-client`, then exposes user-shaped views,
snapshots, typed actions, action metadata, progress, issues, logs, and project
summaries. UI code should render this language and dispatch actions; it should
not drain service effects, route provider runtimes, or correlate protocol
responses itself.

## Boundaries

- `lpa-link` owns provider resources such as browser workers, endpoint/session
  identity, and device/runtime management.
- `lpa-client` owns server protocol request ids, response correlation, typed
  project operations, and side-channel protocol events.
- `lpa-studio-ux` owns Studio product state and async action execution above
  those services.
- `lpa-studio-web` renders `StudioView` panes and available actions.

The first slice supports the browser-worker simulator and browser Web Serial
ESP32 entrypoints. It launches `fw-browser` through `lpa-link`, talks to the
real `lp-server` protocol through `lpa-client`, attaches to a running project
when one is already loaded, can load the demo project, and reads project
inventory.

The old `lpa-studio-core` / `lpa-studio-runtime` split has been removed from
the active workspace. The UX crate owns the controller logic directly instead
of routing application work through a separate effect/event executor.

## Validation

```bash
cargo check -p lpa-studio-ux
cargo test -p lpa-studio-ux
cargo check -p lpa-studio-ux --target wasm32-unknown-unknown --features browser-worker,browser-serial-esp32
```
