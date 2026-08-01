# ADR: Runtime node-command channel (the first non-overlay client→engine write)

- **Status:** Accepted
- **Date:** 2026-07-27
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-07-26-node-card-faces.md` (names the activate-entry
  gap); `2026-07-14-wire-hello-versioning.md` (the version bump this
  change spends); `2026-07-04-studio-editing-model.md` (the overlay write
  path this channel deliberately is NOT)

## Context

Until now, every client→engine write was an overlay mutation: stage an
edit, watch it in the Save panel, commit it to a def artifact. That is
the right shape for *authoring* — edits are durable statements about what
the project should be — but the node-card playlist face needs a gesture
that is not an edit at all: **click a non-active entry, and the playlist
switches to it now**. The engine already has the exact entry point
(`PlaylistNode::switch_to(entry, time)`, the same function trigger
messages and timed advance call); what was missing is any wire path that
reaches live runtime state without going through the overlay
(2026-07-26-node-card-faces.md records the gap explicitly).

The same need is already visible further out: a sim button press must
poke a runtime trigger, and debug tooling wants one-off runtime pokes
(force a produce, dump a runtime slot). None of those are edits either.

## Decision

Add a **runtime command channel**: an honest, non-overlay request path
from client to a live node runtime.

```
WireProjectCommand::NodeCommand { node: NodeId, command: WireNodeCommand }
        → lpa-server Project::node_command
        → Engine::handle_node_command (frame-time stamped)
        → NodeRuntime::handle_command(&mut self, cmd, time_s)   // default: reject
        → PlaylistNode: validate key, queue; next produce → switch_to
```

- **A command is a poke, not an edit.** Nothing is staged in the overlay,
  nothing appears in the Save panel, nothing persists. The effect lands
  on live runtime state and is observed through ordinary project reads
  (the strip's ACTIVE placard follows on the next refresh).
- **Rejection is a normal response.** Unknown node, dead runtime,
  unsupported command, out-of-range payload → `Rejected { reason }`, a
  data-level answer. The request envelope never errors for these and the
  node's runtime status is untouched — a stale click cannot poison the
  connection or the card.
- **Opt-in per runtime.** `NodeRuntime::handle_command` defaults to
  rejecting everything; a node kind that supports commands overrides it.
  The playlist validates the entry key immediately but defers the switch
  to its next `produce`, in the consumed `time` slot's domain, so a
  command switch resets the entry clock exactly as a trigger switch does
  (scrubbed/rated clocks included). Latest command wins within a frame,
  and an explicit activate beats a same-frame trigger message.

### One command enum, growth policy

`WireNodeCommand` (`lpc-wire/src/project_command/node_command.rs`) is ONE
externally-tagged enum — one variant per command, payloads inline. Future
consumers (sim button press, debug pokes) add **variants**, not new
`WireProjectCommand` arms and not per-kind sub-enums, until a real
namespace collision forces the split. Every variant change is a breaking
wire change: bump `WIRE_PROTO_VERSION`.

### Entry-click semantic

On the playlist face strip, **clicking a non-active entry activates it
now** — a runtime poke, nothing staged. Every entry chip carries the
activate action, mounted child or not (activation addresses the
entries-map key, which exists independent of child mounting). The
**ACTIVE entry's chip instead keeps the child select/Focus action**:
activating what is already playing is a no-op, and the click is more
useful as "take me to the (rendered) child card". Child selection for
non-active entries stays available via the card header/title affordances.

The studio client resolves the playlist's CURRENT runtime `NodeId` from
the stable authored address at dispatch time (`PlaylistActivateOp`), so a
queued click can never address a stale runtime id across a reload.

### WIRE_PROTO_VERSION 1 → 2

Growing `WireProjectCommand` is a breaking wire change under the hello
contract's hand-bumped-integer rule, so this change bumps
`WIRE_PROTO_VERSION` to 2. The gate is hard equality: **every fielded
device must be reflashed** to speak to a Studio built from this commit.
That is sanctioned — the no-wire-compat heavy-dev policy (AGENTS.md)
ships client, server, and firmware in lockstep, and the mismatch UX is
already the `Incompatible` state + reflash affordance from the hello ADR.

## Consequences

- The overlay stops being the only write path, and the two paths have
  clean, opposite contracts: overlay = durable authored state, Save-gated
  and revision-bumped; commands = ephemeral runtime pokes, immediate and
  unrecorded. Neither leaks into the other's UI (no phantom Save-panel
  rows for clicks; no runtime pokes silently mutating defs).
- `NodeRuntime` grows one defaulted method; existing node kinds are
  untouched.
- Firmware inherits the enum growth by compiling `lpc-wire` — no
  firmware-side feature work, but the wire battery (fw-esp32/fw-emu
  checks, `scene_render_emu`) must run on every command-vocabulary
  change.
- Commands are fire-and-observe: `Accepted` means "queued at the
  runtime", and the visible effect arrives through the ordinary read
  loop. There is no per-command result payload yet; if a future command
  needs one, it belongs in the response variant, not in a side channel.

## Alternatives Considered

**Synthesize a trigger-slot overlay edit** (stage a mutation on the
playlist's trigger machinery and let the engine's existing trigger path
switch entries). Rejected: it pollutes the Save panel with a row for
something that is not an edit, forces an immediate-commit-then-revert
dance to avoid persisting it, and its semantics are dishonest — the
project def would transiently claim an authored trigger the user never
wrote. The trigger path (`detect_triggered_entry` / `last_seen_triggers`)
stays untouched and message-driven.

**Per-kind command enums or a new top-level wire message.** Rejected as
premature structure: one flat enum keeps the dispatch honest and the
serialization externally tagged (repo lint); split only when a real
collision appears.

## Follow-ups

- Sim button press and debug pokes adopt the channel as new
  `WireNodeCommand` variants.
- Live walk (orchestrator/Yona): click entry → switches; timed advance
  still works; wrong-index rejected gracefully.
