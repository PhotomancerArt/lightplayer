# lpa-studio-ux

`lpa-studio-ux` is the experimental UI-independent Studio UX layer.

`Ux` means a resource-owning product surface. The crate owns lower-level
services such as `lpa-link` and `lpa-client`, then exposes user-shaped
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
- `lpa-studio-web` renders snapshots and available actions.

The first slice supports the browser-worker simulator. It launches `fw-browser`
through `lpa-link`, talks to the real `lp-server` protocol through
`lpa-client`, loads the demo project, and reads project inventory.

The older `lpa-studio-core` and `lpa-studio-runtime` crates remain in the
workspace as references during this experiment.

## Validation

```bash
cargo check -p lpa-studio-ux
cargo test -p lpa-studio-ux
cargo check -p lpa-studio-ux --target wasm32-unknown-unknown --features browser-worker
```
