# Milestone 3: Legacy source migration

## Goal

Move the legacy shader / fixture / output source model onto the core source
path: TOML artifacts and `lpc-source` shapes instead of legacy `node.json`
configs and legacy config trait objects.

This milestone is about authored data and loading, not full runtime cutover.

## Context

The old engine path loads per-node `node.json` files into legacy config trait
objects. The new runtime-core direction prefers update-in-place inside the
`lpc-*` family, with source artifacts and node configs flowing through
`lpc-source`.

The core engine needs a source format it can load consistently before legacy
runtime behavior is ported onto it.

## In scope

- Define TOML-backed source artifacts/configs for the legacy MVP slice:
  - shader;
  - fixture;
  - output;
  - texture compatibility only if required by the shader -> fixture path.
- Map legacy config fields to `lpc-source`/`lpc-model` shapes.
- Decide whether migrated legacy source types live as:
  - `legacy` modules inside `lpc-source`/`lpc-model`; or
  - directly named core source artifacts.
- Add loading tests for TOML source artifacts.
- Preserve compatibility or migration notes for existing `node.json` fixtures
used by tests/examples.
- Keep `LegacyProjectRuntime` available while this source migration lands.

## Out of scope

- Porting node runtime implementations onto the new engine.
- Retiring JSON loading everywhere in one step if a temporary compatibility path
is still needed.
- Retiring `LegacyProjectRuntime`.
- Render products beyond current texture-backed compatibility.

## Key decisions

- **Source migration before runtime port:** make the data path clear before
asking runtime nodes to implement the new contracts.
- **Compatibility should be deliberate:** if `node.json` remains temporarily,
document whether it is a compatibility loader or test fixture only.
- **Avoid new domain stacks:** do not create parallel `lpl-source` or
`lpl-wire` layers unless this milestone proves the core family cannot hold
the migrated shapes.

## Suggested plan location

When ready, expand this milestone with `/plan` or `/plan-small` at:

`docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/`

## Success criteria

- The legacy MVP source shape can be authored and loaded as TOML.
- The loaded source produces `SrcNodeConfig` / artifact references usable by the
core engine.
- Existing behavior has a clear compatibility/migration story.
- The next milestone can focus on runtime behavior instead of source format.