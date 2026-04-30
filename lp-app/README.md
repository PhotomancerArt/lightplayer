# lp-app

Application-facing LightPlayer crates.

`lp-app` is about actually running LightPlayer from the outside: servers,
clients, transports, command/app integration, and demos that coordinate one or
more `lpc-engine` instances.

The engine internals live in `lp-core`. App crates should depend on the core
crates they need, but they should not become the home for core engine concepts
like source schema, wire model definitions, resolver state, or node runtime
logic.

## Crates

- `lpa-server` — embeddable server layer for hosting engines, managing
  projects, and serving the `lpc-wire` API over app-provided transports.
- `lpa-client` — client-side transport/API layer for talking to a LightPlayer
  server or firmware target.
- `web-demo` — browser demo and tooling for the shader pipeline.

## Boundary

Use `lp-app` when code is about process/app behavior: opening a connection,
serving requests, routing messages, coordinating project lifecycles, or wiring
LightPlayer into a CLI, firmware, web app, or host application.

Use `lp-core` when code is about one engine's model, source format, runtime,
wire contract, or local view cache.
