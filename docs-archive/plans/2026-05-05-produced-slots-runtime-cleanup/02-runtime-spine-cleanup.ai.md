# Runtime Spine Cleanup

## Scope of phase

Make the demand-driven `Node` trait the one obvious runtime spine and remove
old runtime machinery that is no longer used by project initial load.

In scope:

- Delete or quarantine the old `NodeRuntime` trait.
- Remove exports that present `NodeRuntime` as a supported runtime API.
- Delete old legacy `runtime.rs` implementations when they are unused.
- Preserve shared helper modules used by the new core nodes.
- Update docs/tests that only asserted `NodeRuntime` compatibility.

Out of scope:

- Changing node behavior.
- Redesigning produced-slot access; that happens in the next phase.
- Removing texture nodes/artifacts.

## Code organization reminders

- Keep `Node` docs focused on current runtime semantics.
- Do not leave commented-out old runtime code.
- Preserve helper modules that core nodes still import.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-engine/src/node/node.rs`
- `lp-core/lpc-engine/src/nodes/node_runtime.rs`
- `lp-core/lpc-engine/src/nodes/mod.rs`
- `lp-core/lpc-engine/src/lib.rs`
- `lp-core/lpc-engine/src/legacy/nodes/**/runtime.rs`
- `lp-core/lpc-engine/tests/runtime_spine.rs`
- `lp-cli/src/commands/profile/symbolize.rs`

Expected changes:

- Remove the public `NodeRuntime` trait unless an actual current caller
  requires it.
- If old `legacy/nodes/*/runtime.rs` files are unreachable, remove the modules
  and tests that only boxed old runtimes.
- Update `runtime_spine` to assert the new `Node` spine rather than legacy
  reachability.
- Update comments and symbolization fixtures that mention the old runtime only
  as historical naming.

## Validate

```bash
cargo test -p lpc-engine --test runtime_spine
cargo test -p lpc-engine
```
