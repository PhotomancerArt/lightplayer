# Phase 2: Mockup Mini Stack

## Scope Of Phase

Reshape `lpc-slot-mockup` into a miniature LightPlayer stack that pressures the real slot model.

In scope:

- `model`, `source`, `engine`, `wire`, and `view` modules,
- source defs that resemble real project/shader/fixture/output/texture defs,
- engine runtime nodes/state materialized from source defs,
- shape registration for source and engine roots.

Out of scope:

- changing real `lpc-source`, `lpc-engine`, `lpc-wire`, or `lpc-view`,
- implementing the derive macro.

## Code Organization Reminders

- Keep each main concept in its own file.
- Separate authored source objects from engine-owned runtime objects.
- Keep wire/view code free of concrete source/engine type dependencies.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

- Replace the current mockup `domain/server/client/sync` organization with:
  - `model`: shared domain/editor enums and helpers,
  - `source`: `ProjectDef`, `ShaderDef`, `ShaderParamDef`, `FixtureDef`, `OutputDef`, `TextureDef`,
  - `engine`: `MockRuntime`, `ShaderNode`, `FixtureNode`, `OutputNode`,
  - `wire`: full sync, patch, diff, snapshot, traversal,
  - `view`: generic client mirror.
- Make `ShaderDef` include `glsl_path`, `texture_loc`, `render_order`, compiler options, and `param_defs: SlotMap<String, ShaderParamDef>`.
- Make `ShaderNode` own runtime params materialized dynamically from `ShaderDef`.
- Include fixture config/state with an enum mapping, an option, and a map with removable stable keys.

## Validate

```bash
cargo test -p lpc-slot-mockup
```
