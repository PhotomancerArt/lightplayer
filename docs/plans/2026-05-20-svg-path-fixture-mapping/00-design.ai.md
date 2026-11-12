# SVG Path Fixture Mapping Design

## Scope Of Work

Add a small `SvgPath` authored fixture mapping that is resolved at project load time into existing
fixture mapping points. This is intentionally a narrow bridge for the FYeah sign, not a general SVG
layout system.

The implementation should:

- Let fixture TOML reference an SVG file.
- Extract only top-level mapping groups with one path-like element and one text label.
- Parse `path:N,count:N`.
- Reject any mapping-looking text outside a valid mapping group.
- Sort by path number and assign output channels end-to-end.
- Fit SVG coordinates into the fixture render size without stretching.
- Keep the render/precompute hot path free of SVG text parsing.

## File Structure

```text
lp-core/lpc-model/src/nodes/fixture/
  mapping.rs

lp-core/lpc-engine/src/nodes/fixture/mapping/
  mod.rs
  points.rs
  svg_path/
    mod.rs
    svg_path_error.rs
    svg_path_fit.rs
    svg_path_group.rs
    svg_path_parser.rs
    svg_path_data.rs

lp-core/lpc-engine/src/engine/
  project_loader.rs

examples/fyeah-sign/
  project.toml
  fixture.toml
  output.toml
  clock.toml
  radio.toml
  playlist.toml
  idle.toml
  active.toml
  idle.glsl
  active.glsl
  fyeah-mapping.svg
```

Use the exact final example directory only if it fits the current example naming. Extending
`examples/button-sign` is acceptable if that is less churn.

## Architecture Summary

`MappingConfig::SvgPath` is an authored source reference. It does not directly feed fixture
precomputation.

At project load time:

1. `ProjectLoader` sees a fixture whose `mapping` is `SvgPath`.
2. It resolves `source` relative to the fixture TOML file.
3. It reads the SVG as UTF-8.
4. It calls the small SVG path resolver.
5. The resolver returns a runtime-friendly `MappingConfig::PathPoints` or equivalent compact
mapping made from explicit point-list paths.
6. `FixtureNode::new` receives the resolved mapping, so existing render and output behavior remains
unchanged.

At render/precompute time:

1. `generate_mapping_points` handles existing `RingArray` paths.
2. It also handles the new explicit point-list path spec, assigning channels in the order already
encoded by the resolved mapping.
3. `compute_mapping` and accumulation continue to work through `MappingPoint`.

## Main Components And Interactions

### Model Mapping Shape

Extend `lp-core/lpc-model/src/nodes/fixture/mapping.rs`.

Add authored mapping:

```rust
MappingConfig::SvgPath {
    source: SourcePathSlot,
    sample_diameter: PositiveF32Slot,
}
```

Add a runtime path spec for explicit lamp positions:

```rust
PathSpec::PointList {
    first_channel: ValueSlot<u32>,
    points: MapSlot<u32, XySlot>,
}
```

The point map key is the zero-based point index inside the path. `first_channel` lets resolved SVG
paths preserve explicit contiguous channel offsets after sorting by `path:N`.

### SVG Parser

Add a tiny parser in `lpc-engine`, not a dependency-heavy XML/SVG stack.

Supported SVG subset:

- Root `viewBox` is parsed when present.
- Top-level `<g>...</g>` blocks only.
- Groups containing nested `<g` are ignored or rejected with a clear error if they otherwise look
  like mappings.
- A mapping group must contain exactly one path-like element:
  - `<path d="...">`
  - `<polyline points="...">`
- A mapping group must contain exactly one text payload whose normalized text content is
  `path:N,count:N`.
- Any `<text>` payload whose normalized content starts with `path:` must be inside a valid mapping
  group.
- Text may be inside `<text>` and `<tspan>`.
- Attribute order is irrelevant.
- Quote style should support both single and double quotes if cheap.
- Comments, defs, style, circles, outlines, posts, and ungrouped geometry are ignored.

Supported geometry:

- `polyline points="x y x y ..."` and comma variants.
- Path commands `M/m`, `L/l`, `H/h`, `V/v`.
- Curves are not supported in the MVP. `C/c`, `S/s`, `Q/q`, `T/t`, `A/a`, and other unsupported
  commands inside mapping groups return an error naming the group path number if known.

### Aspect-Fit Coordinates

Fit source SVG coordinates into the fixture texture coordinate space without stretching.

The simplest normalized form:

- Determine source bounds from `viewBox` when present; otherwise use mapping geometry bounds.
- Determine destination aspect from `render_size.width / render_size.height`.
- Scale uniformly so the full source bounds fit inside the destination rectangle.
- Center the fitted result on the shorter axis.
- Emit normalized coordinates in `[0, 1]`.

For example, a wide SVG mapped to a square texture will use the full width and get vertical padding.

### Lamp Sampling

For each sorted group:

- Flatten path-like geometry to a polyline.
- Reject zero-length paths.
- If `count == 1`, place the only lamp at the start point.
- If `count > 1`, sample distance positions `i / (count - 1)` so the first lamp is exactly at the
  start and the last lamp is exactly at the end.
- Store sampled points as a `PointList` path with `first_channel` equal to the accumulated channel
  offset.

Validation:

- `path` must be unique.
- `count` must be greater than zero.
- At least one mapping group must be found.
- Mapping groups must sort deterministically by `path`.
- Duplicate path numbers are errors.
- Text starting with `path:` outside a valid top-level group is an error.
- SVG file read and UTF-8 errors should include the source path.

## Non-Goals

- Do not support transforms yet.
- Do not support nested groups yet.
- Do not support arbitrary SVG elements.
- Do not support curve commands in this temporary importer.
- Do not pull in a broad SVG rendering stack.
- Do not make the compiler or runtime shader path optional.
- Do not precompile fixture mappings on the host as a product requirement.
