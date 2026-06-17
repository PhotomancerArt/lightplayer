# fw-browser

`fw-browser` is the browser/Web Worker LightPlayer runtime target.

It exists for Studio simulation and browser-local project testing. It is not the
embedded product path and it is not a replacement for ESP32 runtime shader
compilation. The browser runtime still uses the real shader frontend and
`lpvm-wasm` browser backend to compile and execute shaders in the browser.

## Relationship To Other Crates

- `lps-frontend` parses and lowers GLSL.
- `lpvm-wasm` compiles the lowered shader to wasm and runs it through browser
  `WebAssembly` APIs.
- `lpa-link` `local-browser` models browser runtime instances and scoped
  logs/status for Studio.
- Future Studio UI code should consume this through a browser-local link/session
  boundary rather than reaching directly into shader runtime details.

## Public Proof Surface

The current wasm-bindgen exports are intentionally small:

- initialize browser builtin exports
- create a named runtime instance
- compile a shader into that runtime
- render the first pixel
- read runtime-scoped logs
- read runtime count

That proves the first browser-local thread without committing to the final
Studio API.

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
`document.documentElement.dataset.smoke == "ok"`, and renders a red test pixel.

`just fw-browser-test` runs the Rust-native `wasm-bindgen-test` path. It requires
a working browser/WebDriver environment, so local failures caused by missing or
broken browser automation should be treated as runner provisioning issues.
