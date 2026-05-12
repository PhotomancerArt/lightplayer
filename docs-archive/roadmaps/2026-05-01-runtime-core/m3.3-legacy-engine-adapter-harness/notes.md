# M3.3: Legacy Engine Adapter Harness

## Purpose

This milestone is intentionally superseded by M4.

After discussion, we decided not to build a temporary legacy adapter harness.
The old nodes are useful nodes, and the goal is to rework them into first-class
core `Node` implementations rather than wrap old runtime APIs just to remove
the wrappers later.

## Decision

- Skip M3.3 as an implementation milestone.
- Let M4 make a clean break, even if the branch is broken while the port is in
  progress.
- Treat `LegacyProjectRuntime` as reference code available in git/worktrees, not
  as an active compatibility layer to preserve.
- Port texture, shader, fixture, and output as durable core-engine nodes.
- Use M4 to validate the new engine concepts directly: runtime buffers, products,
  demand roots, and source-to-engine construction.

## Handoff To M4

M4 owns the source-to-engine construction path and the first-class node ports.
The old runtime may remain in the tree temporarily, but new work should not add
adapter scaffolding whose only purpose is preserving the old runtime shape.
