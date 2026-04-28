# Milestone 5: Height-one palette lookup and lp-shader API integration

## Goal

Make height-one textures practical for palette/gradient lookup and integrate the
texture binding contract into the high-level `lp-shader` API used by lpfx.

## Suggested plan location

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m5-height-one-palette-api/`

Small plan: `plan.md`.

## Scope

### In scope

- Enforce and expose `TextureShapeHint::HeightOne` as a first-class optimization
  promise in the runtime API.
- Add a height-one optimized sampling helper/path for palette/gradient use.
  The exact shader surface should be settled in the small plan:
  - a documented GLSL helper builtin, or
  - optimized lowering of `texture()` when the descriptor is `HeightOne`, or
  - both if the implementation naturally supports both.
- Add `lp-shader` runtime helpers for constructing texture uniform values from
  `LpsTextureBuf` without callers hand-authoring descriptor structs.
- Update `LpsPxShader`/`render_frame` API shape as needed so lpfx can provide
  texture uniforms cleanly.
- Add `lp-shader`-level tests that compile a shader, bind an input
  `LpsTextureBuf`, and render/read expected results through the public API.
- Document expectations for higher layers:
  - palette stop baking is not in `lp-shader`,
  - lpfx/domain provide the `HeightOne` descriptor and matching texture.

### Out of scope

- Palette stop interpolation/baking from TOML/domain values.
- lp-domain schema changes.
- wgpu source/runtime support.
- New texture formats.

## Key decisions

- Height-one is a shape hint on a 2D texture, not a separate 1D resource type.
- Runtime `height != 1` with `HeightOne` is a hard error.
- Public APIs should expose texture helpers/values, not raw pointer structs.

## Deliverables

- Height-one optimized sampling behavior.
- Public helper(s) for texture uniform binding from `LpsTextureBuf`.
- `lp-shader` API tests for texture input binding and palette-like lookup.
- Documentation notes for lpfx/domain consumers.

## Dependencies

- Depends on Milestone 4 for normalized `texture()` sampling and filter policy.
- Depends on Milestone 3c for exact fetch machinery and runtime texture
  validation.

## Execution strategy

**Option B — Small plan (`/plan-small`).**

Justification: The core texture ABI, fetch, filtering, and filetest foundation
are already established by this point. This milestone mostly packages
height-one behavior and public API integration for consumers, with one focused
surface question for the helper shape.

**Suggested chat opener:**

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?

