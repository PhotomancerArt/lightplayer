# M4.5 notes: core node state model

## Why this exists

M4.1 needs to restore detail sync quickly enough to make the core runtime useful
in the demo, but that does not mean the legacy `NodeState` hierarchy is the
right long-term representation for core nodes.

Resource refs make the issue obvious: a fixture's `lamp_colors` field has a
semantic name and should point at a buffer/resource, but embedding that into old
heavy snapshot structs risks locking in the wrong model.

## Current pressure

- Legacy `NodeState` variants are client-visible and already wired through
  `ProjectView`.
- M4.1 likely needs compatibility fields or adapters so state/detail sync works.
- `RuntimeStateAccess` exists but is marker-only.
- Node details need semantic field names, not an unkeyed list of resource refs.
- Value-domain state and resource-domain state may need different sync rules.

## Planning questions

- Should core node state be a typed enum, a field-path map, trait-projected
  state, or node-specific detail structs?
- Should compatibility `NodeState` be renamed or isolated under a legacy module?
- How should field-level versions work for semantic value/resource fields?
- Which data belongs in user-facing details versus debug/runtime state?
- How should `RuntimeStateAccess` expose state without becoming another broad
  `RuntimePropAccess`?

## M4.1 compatibility rule

Any compatibility state additions made in M4.1 should be named or placed so they
are easy to find later. Prefer names/docs that include `legacy` or
`compatibility` when the shape exists only to bridge current wire/view behavior.

