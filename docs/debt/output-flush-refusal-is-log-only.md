---
status: carried
since: 2026-09-02
logged: 2026-09-07
area: lpc-engine output flush + lpa-server tick + lpc-wire project read
related:
  - "../adr/2026-09-02-fault-is-never-black.md"
  - "espressif-devkit-led-wire-label-mismatch.md"
---
# An output wire the board refuses is a log line and nothing else

**Shape** — The refusal itself is honest and specific. A project
authored for a wire the worn board manifest does not declare fails at
open: `VirtualWs281xDriver::gpio_for_endpoint`
(`lp-core/lpc-hardware/src/drivers/ws281x/virtual_ws281x_driver.rs:126`)
returns `HardwareEndpointError::UnknownEndpoint`,
`EngineServices::flush_dirty_output_sinks` wraps it as
`OutputFlushError::Provider`
(`lp-core/lpc-engine/src/engine/engine_services.rs:711`) — *"output node
3 port 0 ws281x:local:D9: Invalid config: unknown Ws281x hardware
endpoint"* — and `Engine::tick` returns it as
`EngineError::OutputFlush` (`lp-core/lpc-engine/src/engine/engine.rs:666`),
endpoint string intact.

It dies one layer up. `LpServer::advance_frame`
(`lp-app/lpa-server/src/server.rs:577`) turns that error into a
`log::warn!` and a per-handle failure **count**, then returns `Ok(())`.
The count has no reader but the log throttle; the message is not kept
at all. Its own comment — *"clients see it when they sync or query
project state"* — is false today.

Nothing downstream picks it up. The flush runs OUTSIDE the node walk,
and every write to `RuntimeNodeEntry.status` comes from the walk
(`engine.rs:1568`, `:2285`, `:2381`, `:2476`, `:2574`) or from load
(`project_loader.rs:942`), so a refused wire never reaches
`NodeRuntimeStatus`: the output node reports `Ok`, exactly as it would
with the wire open. `ProjectRuntimeStatus`
(`lp-core/lpc-wire/src/messages/project_read/runtime_read.rs:26`) has no
error field. The heartbeat's `LoadedProject.fault` is derived only from
`NodeRuntimeStatus::Fault`, and fw-browser emits no heartbeat anyway.
`EngineServices` keeps a private `OutputWire.parked_at_generation`
(`engine_services.rs:78`) and exposes no getter for it. `ServerMsgBody::Log`
exists on the wire and no code in `lpa-server` ever sends one.

So the only report is `log::warn!` — the worker JS console under
fw-browser, the UART under firmware. Studio's `<label> unresolved`
badge (`lp-app/lpa-studio-web/src/app/node/face/output_face.rs:525`)
looks like the missing signal but is not: it is a client-side lookup of
the authored pin label against a Studio-local `lpa_boards` manifest
keyed by an advisory `sim_board_id`, and it never asks the server
anything. A real board that refuses a wire Studio's local manifest
declares shows no badge at all.

**Carrying cost** — The strictness a board sim exists to provide is
invisible to the surface that would use it. A project loaded onto the
wrong board renders correctly in the preview and drives nothing, with
no badge, no fault, no message — the user's only signal is dark LEDs.
It also makes the behaviour untestable from any client: PR #579's
fw-browser test tried to assert the refusal through
`NodeRuntimeStatus` and found `Ok`, and its sibling
`desktop_boot_accepts_a_dome_wire_label` had been passing vacuously for
the same reason (`Ok` reads the same either way). Every future test of
board strictness has to be written at the engine-services seam or
below, and every future UI wanting to say "this wire is not on this
board" has to add the seam first.

**Workarounds** — Assert refusals at the seam that carries them:
`EngineServices::flush_dirty_output_sinks` returns the named error
(`a_failing_output_sink_does_not_suppress_the_others`,
`engine_services.rs:1219`), and `HardwareSystem::open_ws281x_by_spec`
returns `UnknownEndpoint`
(`virtual_system_reports_unknown_ws281x_endpoint_spec`,
`lp-core/lpc-hardware/src/hw_system.rs:345`). Board tables are covered
by `default_desktop_manifest_opens_every_catalog_wire_label_at_once`
(`lp-core/lpc-hardware/src/manifest/default_manifests.rs:306`). When
diagnosing a live sim, read the worker console for
`EngineServices: output node … : Invalid config: unknown Ws281x
hardware endpoint`. Note the parking contract when reproducing: the
error is returned on the FIRST failing frame only, then the wire parks
until the hardware generation changes (`ensure_port_open`,
`engine_services.rs:865`).

**Incident log**

- 2026-09-07 — PR #579 (plan "Always a device", P1). CI job *Validate
  Browser (x64)* red on
  `board_manifest_boot_refuses_an_endpoint_the_board_lacks`: the console
  showed the engine refusing `ws281x:local:D9`, and
  `output_node_status` returned `Ok` in the same breath. The test was
  rewritten to assert the boot wiring and the two costs the refusal is
  allowed to have (no wedge, no black frame), with the `Ok` pinned
  against this entry.

**Exit criteria** — A wire client can name a refused endpoint from a
query it can make. Concretely: the output node carries a `Warn`/`Error`
status naming the endpoint it could not open (the `output_smoothing_notice`
path, `engine_services.rs:357` → `TickContext` →
`OutputNode::runtime_status`, is the precedent for carrying a
services-side fact into a node status), OR a tick error is retained and
readable through `ProjectReadQuery::Runtime`. When it lands, the pinned
`Ok` assertions in `lp-fw/fw-browser/src/tests.rs` break and are
upgraded to assert the named refusal, and `server.rs:565`'s comment
becomes true.
