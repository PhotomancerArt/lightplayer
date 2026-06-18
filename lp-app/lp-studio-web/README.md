# lp-studio-web

`lp-studio-web` is the first static browser shell for LightPlayer Studio.

It renders `lp-studio-core` state and drives the browser-local `browser-worker`
runtime path from `lp-studio-runtime`. It does not own Studio domain behavior and
does not use Dioxus server functions.

## Run

```bash
just studio-web-build
just studio-web
```

`studio-web-build` builds the Dioxus web app with Cargo, packages it with
wasm-bindgen, and prepares the `fw-browser` worker assets used by the Studio demo
flow.

## Boundary

- `lp-studio-core` owns actions, state, effects, diagnostics, and sessions.
- `lp-studio-runtime` owns browser worker protocol flow and demo project loading.
- `lp-studio-web` owns Dioxus components and static presentation.
