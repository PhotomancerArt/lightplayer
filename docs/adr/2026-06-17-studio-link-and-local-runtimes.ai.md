# ADR: Studio Link And Local Runtimes

- **Status:** Accepted
- **Date:** 2026-06-17
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

LightPlayer Studio needs to discover and manage local and physical LightPlayer
endpoints without putting low-level device and runtime concerns directly in the
UI. The first Studio milestone also needs a browser-local runtime so the web app
can prove the end-to-end flow before real hardware flashing is available.

The low-level layer is broader than a byte transport. Web Serial, browser-local,
host-local, and later websocket/server-owned links may all need discovery,
status, reset, flash, raw filesystem access, diagnostics, logs, and eventually a
client connection to an `lp-server`.

Browser-local and host-local runtimes also have different purposes. A browser
runtime is for Studio project testing and simulation in a Web Worker-shaped
environment. A host runtime is for running LightPlayer on a host OS, server, or
single-board computer.

## Decision

Add `lpa-link` as the app-side low-level link layer below Studio capabilities
and beside `lpa-client`.

`lpa-link` owns provider discovery, endpoint identity/status, low-level
management surfaces, raw logs/diagnostics, and opening a server/client
connection. `lpa-client` remains the typed client/RPC layer once a connection
exists. Studio owns higher-level capabilities, user/agent actions, client
sessions, project sessions, undo, and product workflows above the link layer.

Use separate runtime targets:

- `fw-browser` for browser/Web Worker Studio simulation and project testing.
- `fw-host` for host-OS local runtime use cases.

Local runtime support is plural-first from the start. The type model must allow
multiple browser or host runtime instances so future multi-node and radio-style
LightPlayer systems are not forced through singleton assumptions.

## Consequences

Studio can be driven by both UI code and future agent harnesses through the same
domain surfaces, while `lpa-link` keeps low-level endpoint concerns out of the
UI.

`lpa-link` can grow Web Serial and hardware-management functions without
confusing those functions with typed project/client RPCs.

`fw-browser` and `fw-host` can evolve independently where browser and host
runtime constraints differ. This avoids pretending that browser shader execution,
host process lifecycle, and embedded firmware are the same product surface.

Browser runtime validation needs an explicit ladder: wasm target check,
wasm-bindgen package build, a Rust-native `wasm-bindgen-test`, and browser smoke
coverage. CI-enforced headless browser execution remains dependent on browser
and WebDriver provisioning.

## Alternatives Considered

- Put all local behavior directly in Studio UI code.
  - Rejected because it would entangle UI workflows with endpoint discovery,
    hardware management, logs, and connection lifecycle.
- Treat the new layer as only a transport crate.
  - Rejected because real links own more than `connect()`: discovery, status,
    management, diagnostics, flashing, reset, and raw filesystem access belong
    below Studio capabilities.
- Use one generic local firmware/runtime for browser and host.
  - Rejected because browser and host runtimes have different purposes,
    compilation constraints, output surfaces, and lifecycle models.
- Start singleton-shaped and generalize later.
  - Rejected because multi-instance runtime support is foundational for future
    multi-node LightPlayer systems.

## Follow-ups

- Provision CI/browser tooling for `fw-browser` `wasm-bindgen-test` execution.
- Add Web Serial and real hardware flashing as a later link provider.
- Keep embedded runtime shader compilation intact; browser and host runtimes are
  Studio/local surfaces, not replacements for on-device GLSL JIT compilation.
