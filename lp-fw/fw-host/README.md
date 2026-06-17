# fw-host

`fw-host` is the host-OS LightPlayer runtime target.

It extracts local runtime/server lifecycle out of `lp-cli` so Studio and other
host applications can create local LightPlayer runtime instances without owning
server internals directly.

## Relationship To Other Crates

- `lpa-server` hosts projects and serves the LightPlayer wire API.
- `lpa-client` consumes the client-side connection created by the runtime.
- `lpa-link` `local-host` uses `fw-host` to create runtime instances and expose
  them as low-level link sessions.
- `fw-core` provides the shared transport drain and server tick helpers used by
  the host runtime loop.
- `lpc-*` and `lpfs` provide the model, hardware, shared transport, wire, and
  filesystem pieces used by the hosted server.

`fw-host` is not embedded firmware. It is a valid runtime target for host
deployments and local development, but it must not replace the ESP32 on-device
GLSL JIT product path.

## Current Scope

The current implementation provides an in-memory runtime suitable for M1 Studio
foundation work:

- start a local memory-backed `LpServer`
- produce a local client transport pair
- shut down cleanly
- run multiple memory runtimes concurrently

Persistent host projects, process supervision, external TCP/UDP outputs, and
packaged host deployments are future productization work.

## Validation

```bash
cargo check -p fw-host
cargo test -p fw-host
cargo check -p lpa-link --features local-host
cargo test -p lpa-link --features local-host
```
