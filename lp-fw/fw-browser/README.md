# fw-browser

`fw-browser` is the browser/Web Worker LightPlayer runtime target.

It exists for Studio simulation and browser-local project testing. It is not the
embedded product path and it is not a replacement for ESP32 runtime shader
compilation. The browser runtime still uses the real shader frontend and
`lpvm-wasm` browser backend to compile and execute shaders in the browser, but
shader work happens behind `LpServer` and project loading rather than through
direct public shader calls.

## Relationship To Other Crates

- `lpa-server` owns projects, filesystem protocol handling, and render ticks.
- `fw-core` provides shared runtime drain/tick helpers.
- `lpvm-wasm` is used by `lpc-engine`'s wasm32 graphics backend to execute
  shaders through browser `WebAssembly` APIs.
- `lpa-link` `browser-worker` models browser runtime instances and scoped
  logs/status for Studio.
- Future Studio UI code should consume this through a browser-local link/session
  boundary rather than reaching directly into shader runtime details.

## Worker Boundary

The wasm-bindgen exports are intentionally small and firmware-shaped:

- initialize browser builtin exports
- create a named runtime instance
- send structured envelope JSON to the runtime
- tick the runtime deterministically
- drain structured output envelope JSON
- read runtime count

`lpa-link` owns the Studio browser-worker wrapper under its
`browser_worker` provider. The `fw-browser/www/fw-browser-worker.js` file is a
standalone smoke-page wrapper for this crate's local browser smoke test.

Input envelopes currently include `protocol_in`, `tick`, `start`, `stop`, and
`drain`. `protocol_in` carries a whole `lpc_wire` client JSON frame. Output
envelopes currently include `status`, `log`, and `protocol_out`. `protocol_out`
carries a whole `lpc_wire` server JSON frame.

Automated smoke coverage should load/tick projects through this boundary and
inspect canonical project-read `OutputChannels` resources rather than reaching
directly into shader or output-provider internals.

## Validation

```bash
cargo check -p fw-browser --target wasm32-unknown-unknown
cargo test -p fw-browser --target wasm32-unknown-unknown --no-run
just fw-browser-build
```

To manually run the browser smoke page:

```bash
just fw-browser-smoke
```

Then open:

```text
http://127.0.0.1:2819/smoke.html
```

Success means the page reports `ok`, sets
`document.documentElement.dataset.smoke == "ok"`, writes a small project through
worker protocol messages, loads it, ticks the worker-owned firmware runtime, and
observes increasing `OutputChannels` bytes through project-read resources.

`just fw-browser-test` runs the Rust-native `wasm-bindgen-test` path. It requires
a working browser/WebDriver environment, so local failures caused by missing or
broken browser automation should be treated as runner provisioning issues.
