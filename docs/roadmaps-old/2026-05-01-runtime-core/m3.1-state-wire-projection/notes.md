# M3.1: State Wire Projection

## Purpose

Harden the state/config/wire projection story before M4 ports legacy runtime
behavior onto the core engine.

M4 should not have to decide ad hoc how core-engine node state becomes
client-visible legacy state, how config details are applied by clients, or
whether large byte buffers stay in the node state stream.

## Working Scope

- Clarify the current legacy `GetChanges` state/config projection.
- Fix serialization and client-view gaps that would make M4 parity hard to
  validate.
- Define a small projection boundary: runtime-owned products/buffers are not
  assumed to be the same thing as wire node state snapshots.
- Keep transport payload redesign out of scope unless a tiny compatibility type
  is needed to document intent.

## Handoff To M3.2

If M3.1 finds state fields that should become first-class products or buffers,
record their required identity/version/snapshot properties for M3.2 rather than
building the full store here.
