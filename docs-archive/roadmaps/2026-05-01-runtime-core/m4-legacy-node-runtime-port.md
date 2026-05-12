# Milestone 4: Core runtime cutover for MVP demo

## Status

Implemented through phase 9. The server/demo load and tick path now uses
`CoreProjectRuntime`; remaining cleanup and parity work is tracked in M4.1/M5
and the M4 `future.md`.

## Goal

Rework the legacy MVP node behavior into first-class core engine nodes and make
the authored shader -> fixture -> output flow run through the new engine stack
for `just demo`.

This is a clean-break milestone. The branch may be broken while M4 is in
progress. `LegacyProjectRuntime` remains available in git/worktrees as reference
code, but M4 should not preserve old runtime shapes through temporary adapters or
a long-lived dual-runtime switch.

## Context

M2 creates the core engine owner/scheduler. M3 migrates legacy-authored source
toward TOML and `lpc-source`. M3.2 adds runtime buffer products. M3.3 was
intentionally superseded: instead of a temporary adapter harness, M4 ports the
useful old MVP nodes directly into the new system.

This milestone should validate the new engine concepts directly:
source-to-engine construction, durable core node implementations, demand-root
execution, runtime-owned buffers/products, and the server/demo wiring required to
make the current MVP flow run.

## In scope

- Build a source project -> core `Engine` construction path.
- Port the MVP legacy runtime nodes as first-class core `Node` implementations:
  - shader visual/producer;
  - fixture demand root;
  - output flush target;
  - texture compatibility if required by shader/fixture behavior.
- Make fixtures demand roots in the core engine.
- Route child/output/bus reads through engine-owned resolution and products.
- Use render products for shader/texture visual output and `RuntimeBufferStore`
  for non-visual bytes such as fixture colors, output channel bytes, protocol
  payloads, and compatibility snapshots.
- Use the engine per-frame cache so demanded producers run at most once per
  frame.
- Use `Versioned<T>` versions for node-private cache decisions where helpful.
- Preserve existing shader compile/execute behavior, including embedded JIT
requirements.
- Wire `lpa-server` / demo runtime path to the new core runtime for the MVP
  project, accepting temporary breakage during the port.
- Add tests and manual validation around the new path; use old runtime code as
  reference, not as an active compatibility layer.

## Out of scope

- Full removal of `LegacyProjectRuntime`; M5 owns retirement/cleanup after M4
  proves the new demo path.
- Proper runtime buffer sync refs/diffs/client cache; M4.1 owns that.
- Polishing backward-compatible legacy APIs during the port.
- Async/parallel scheduler execution.
- Full visual model beyond the legacy MVP slice.

## Key decisions

- **Clean break over adapter layer:** port useful legacy nodes directly into the
core engine instead of adding temporary wrappers around old runtime APIs.
- **Render-product visual output:** shader/pattern output is modeled as a
render product. Runtime buffers remain for non-visual byte payloads and
temporary compatibility snapshots.
- **Demand-root fixture flow:** fixtures drive the frame; outputs flush after
fixture-side mutation.
- **Broken branch is acceptable:** this milestone may temporarily break demo and
tests while the runtime path is replaced.
- **Demo cutover happens here:** M5 is cleanup/hardening, not the first moment
the demo switches to core runtime.

## Suggested plan location

When ready, expand this milestone with `/plan` or `/plan-small` at:

`docs/roadmaps/2026-05-01-runtime-core/m4-legacy-node-runtime-port/`

## Success criteria

- Authored legacy source projects can construct a core `Engine`.
- The core engine can run the MVP shader -> fixture -> output flow.
- `lpa-server` / `just demo` use the new core runtime path for the MVP project.
- Shader/texture visual output lives in runtime-owned render products, while
  fixture/output byte data lives in runtime-owned buffers where practical.
- Producer work is demand-driven and same-frame cached.
- Old runtime code may still exist, but it is no longer the active demo path.