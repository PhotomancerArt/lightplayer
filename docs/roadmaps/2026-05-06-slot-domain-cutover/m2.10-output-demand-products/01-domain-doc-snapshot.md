# Phase 0: Domain Doc Snapshot

## Scope Of Phase

Capture the current LP Core domain model in `docs/lp-core` before the runtime
implementation proceeds.

In scope:

- Add a concise overview document.
- Add one small document per core concept:
  - nodes;
  - slots;
  - values;
  - bindings;
  - resources;
  - products.
- Explain the target shader -> fixture -> output flow.
- Write for a senior engineer or LED-domain expert learning the LightPlayer
  runtime model.

Out of scope:

- Full API reference.
- Long historical explanation.
- Detailed wire/client protocol design.
- Implementation changes.

## Code Organization Reminders

- Keep docs small and scannable.
- Prefer separate files over one large domain document.
- Use terms that match the current direction: `VisualProduct`,
  `ControlProduct`, slots, values, bindings, resources.
- Mark uncertainty plainly when a concept is still evolving.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope into implementation.
- Do not rewrite roadmap history outside this milestone unless needed for links.
- Report what changed and any domain terms that still feel uncertain.

## Implementation Details

Create or update:

- `docs/lp-core/overview.md`
- `docs/lp-core/nodes.md`
- `docs/lp-core/slots.md`
- `docs/lp-core/values.md`
- `docs/lp-core/bindings.md`
- `docs/lp-core/resources.md`
- `docs/lp-core/products.md`

The docs should cover:

- Nodes as runtime ownership/execution units.
- Slots as named, versioned data surfaces.
- Values as payloads inside slot leaves.
- Bindings as consumed/produced slot connections.
- Resources as registry-owned runtime objects.
- Products as lazy graph values materialized on demand.
- The target flow:
  `ShaderNode -> VisualProduct -> FixtureNode -> ControlProduct -> OutputNode`.

## Validate

```bash
find docs/lp-core -maxdepth 1 -type f | sort
```
