# lp-studio-runtime

`lp-studio-runtime` executes `lp-studio-core` effects and turns lower-level
runtime/link/client facts back into Studio events.

## Boundaries

- `lp-studio-core` owns state transitions.
- `lp-studio-runtime` owns I/O, runtime adapters, demo project seeding, and
  client protocol flow.
- `lp-studio-web` owns Dioxus components and browser presentation.

The host-process path is:

```text
StudioEffect -> lpa-link host-process -> fw-host -> lpc-wire protocol
```

The browser-worker path is:

```text
StudioEffect -> lpa-link browser-worker model -> JavaScript Worker -> fw-browser
```

Demo project loading uses the same server protocol on both paths: write files
under `/projects/studio-demo/...`, then call `LoadProject` with
`studio-demo`.

## Validation

```bash
cargo check -p lp-studio-runtime --features host-process
cargo test -p lp-studio-runtime --features host-process
cargo check -p lp-studio-runtime --target wasm32-unknown-unknown --features browser-worker
```
