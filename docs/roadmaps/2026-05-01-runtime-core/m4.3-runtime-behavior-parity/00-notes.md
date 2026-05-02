# M4.3 notes: runtime behavior parity

## Current shortcut

The core path currently assumes an independent `ShaderNode` produces a render
product that a fixture can sample. Legacy behavior can run multiple shaders in
`render_order` into one shared texture/buffer owner. That is the largest
remaining runtime-behavior gap after sync and reload.

Fixture behavior is also only ported for the MVP path. Any parity work should
separate features that are actually used from historical config weight.

## Transform config decision

Do not port fixture transform config as part of M4.3. We do not believe current
projects used it, and carrying it forward now would add runtime and sync surface
without validating a live requirement. Either remove it from the active core
path or keep it as source-only/future compatibility until a real transform node
or visual feature needs it.

## Questions to answer during planning

- Should shared texture ownership live in `CoreProjectLoader`, a render graph
  layer, or a new pattern/target product abstraction?
- Does a texture-target render product eagerly render every shader each frame,
  or lazily render when sampled by a fixture?
- How much old-runtime reference testing is worth keeping before M5 deletion?
- Which fixture config fields are true MVP parity versus future feature ballast?

## Validation focus

- Two shaders targeting one texture respect `render_order`.
- Fixture sampling sees the final shared texture result.
- Nested source discovery works if supported by current authored projects.
- Transform config absence/removal is explicit and tested if old source files can
  still contain the field.
