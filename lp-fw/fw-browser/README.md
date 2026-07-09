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
- render a bus visual product to sRGB RGBA8 pixels (`render_bus_texture_rgba8`,
  the binary preview path — pixels return as a fresh `Uint8Array`, never JSON)

`lpa-link` owns the Studio browser-worker wrapper under its
`browser_worker` provider. The `fw-browser/www/fw-browser-worker.js` file is a
standalone smoke-page wrapper for this crate's local browser smoke test.

Input envelopes currently include `protocol_in`, `tick`, `start`, `stop`, and
`drain`. `protocol_in` carries a whole `lpc_wire` client JSON frame. Output
envelopes currently include `status`, `log`, and `protocol_out`. `protocol_out`
carries a whole `lpc_wire` server JSON frame plus the producing `runtime_id`,
so multi-runtime workers can demultiplex protocol streams.

The `lpa-link` worker wrapper adds worker-level message kinds on top of this
crate's envelopes: `create_runtime` (answered by `runtime_created`) for hosting
several runtimes in one worker, and `preview_frame`, which ticks a runtime and
calls `render_bus_texture_rgba8` in one worker turn. `preview_frame` answers
with a binary `preview_pixels` message whose pixel `ArrayBuffer` is posted as a
transferable — the pixels never ride the JSON envelope path — or with a JSON
`preview_error` envelope on failure. These exist for the Studio preview lab
(dev-only measurement page).

Automated smoke coverage should load/tick projects through this boundary and
inspect canonical project-read `OutputChannels` resources rather than reaching
directly into shader or output-provider internals.

## Clock Ownership (Tick Modes)

The runtime never owns a clock; it advances its `ManualTimeProvider` by exactly
the delta each `tick` envelope carries. Who supplies that delta is a *worker*
concern, selected at boot:

- **Self-ticking** (Studio simulator default): the worker JS runs its own timer
  (~30 fps) and ticks the runtime with the *real* elapsed time measured via
  `performance.now()`. Previews animate at roughly real time even when no
  protocol request is in flight. The Studio client transport is a pure consumer
  of worker output and posts no `tick` envelopes.
- **Explicit** (tests/stories/smoke harnesses): no worker timer runs. Time
  advances only when the host sends a `tick` envelope with a chosen delta. A
  fixed delta gives deterministic advancement, so lockstep tests can pin exact
  frame numbers.

Because the runtime treats the delta opaquely, both modes exercise identical
runtime code; only the *source* of the delta differs. The `lpa-link`
`browser-worker` provider selects the mode through
`BrowserWorkerOptions::tick_mode` (default `SelfTicking`).

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
