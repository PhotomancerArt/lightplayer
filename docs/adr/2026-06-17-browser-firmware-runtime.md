# ADR: Browser Firmware Runtime Boundary

- **Status:** Accepted
- **Date:** 2026-06-17
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The first Studio milestone needs a browser-local LightPlayer runtime that feels
like firmware, not a shader playground. The previous `fw-browser` proof exposed
direct shader/LPVM primitives, which was useful for proving browser shader
execution but too far from the real Studio/device shape.

Studio needs to create browser-local runtimes, send normal LightPlayer protocol
messages, observe logs/status, load projects, tick deterministically in tests,
and inspect output through the same project-read resource path used by other
firmware targets.

## Decision

`fw-browser` is a browser/Web Worker firmware target. It owns an in-memory
`LpServer`, filesystem, virtual hardware, output provider, manual time source,
and server tick loop. JavaScript creates a module Worker and talks to it through
structured `postMessage` envelopes.

Input envelopes include `protocol_in`, `tick`, `start`, `stop`, and `drain`.
`protocol_in` carries a whole `lpc_wire` client JSON frame. Output envelopes
include `status`, `log`, and `protocol_out`. `protocol_out` carries a whole
`lpc_wire` server JSON frame. Logs/status stay separate from protocol frames so
Studio can show connection health and raw protocol independently.

`fw-core` owns only target-neutral runtime helpers: draining client messages and
ticking an `LpServer` frame. Browser Worker lifecycle, host process lifecycle,
and ESP32 scheduling remain target-specific.

`lpa-link browser-worker` models endpoint/session identity and reports a
`BrowserWorker` connection with protocol `fw-browser-post-message-v1`.
The web frontend still owns the actual JavaScript `Worker` object and binds that
worker to Studio/client code.

Output smoke coverage uses canonical project-read `OutputChannels` payloads,
not direct access to `MemoryOutputProvider` and not a bespoke output snapshot.

## Consequences

M1 Studio can depend on a firmware-shaped browser runtime: create a worker,
write project files via protocol messages, load a project, tick, read resources,
and surface logs/status.

Browser and host runtimes remain distinct. `fw-browser` is for Studio
simulation and browser-local project testing; `fw-host` is for host-OS local
runtime deployments.

The current automated Rust wasm check can compile the browser runtime tests, but
executing `wasm-bindgen-test` requires working browser/WebDriver provisioning.
The static browser smoke page is therefore part of the validation ladder until
CI browser tooling is provisioned.

## Alternatives Considered

- Keep direct shader/LPVM exports as the browser API.
  - Rejected because it bypasses `LpServer`, filesystem, project loading, logs,
    and the protocol boundary Studio must use.
- Emulate serial `M!` framing inside the Worker boundary.
  - Rejected for M0a because structured worker messages are simpler and keep
    logs/status/protocol clearly separated. Serial-like framing remains useful
    for serial transports.
- Make `fw-core` own a full runtime factory.
  - Rejected because target-specific lifecycle, logging, scheduling, and
    hardware setup would make `fw-core` too broad too early.
- Verify output through a bespoke worker `outputSnapshot`.
  - Rejected for the primary smoke because canonical project-read resources are
    the surface Studio and agents should be able to trust.

## Follow-ups

- Provision stable CI/browser tooling for `wasm-bindgen-test` or a Playwright
  Worker smoke.
- Add a browser-side `lpa-client` transport/binding for
  `fw-browser-post-message-v1`.
- Add richer diagnostics and optional output snapshots for Studio device panels
  after the canonical protocol path remains stable.

## Later Note

As of the provider-owned resources refactor and the `lpa-studio-ux` experiment,
`lpa-link browser-worker` owns the JavaScript Worker lifecycle. Browser UI code
should consume the Studio UX surface rather than owning the Worker directly.
