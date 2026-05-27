# Phase 3: Loader And Runtime Resolution

## Scope Of Phase

Connect `SvgPath` authored mappings to fixture runtime mapping generation.

In scope:

- Resolve SVG mappings in `ProjectLoader`.
- Read the SVG file relative to the fixture TOML.
- Fit coordinates to fixture render size without stretching.
- Sample lamps along each path-like geometry.
- Convert resolved SVG groups into `PathPoints` with `PointList` path specs.
- Teach `generate_mapping_points` to emit points from `PathSpec::PointList`.

Out of scope:

- Example project changes.
- Broad project read/write UI support beyond the model fields.
- Full validation command suite.

## Code Organization Reminders

- Keep loader glue small in `project_loader.rs`; put SVG-specific work in fixture mapping modules.
- Use existing helper style from `read_shader_source`, `resolve_path_relative_to_file`, and
  `read_utf8_file`.
- Do not put XML parsing in `lpc-model`.
- Keep tests at the bottom of the file they exercise.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-engine/src/nodes/fixture/mapping/points.rs`
- `lp-core/lpc-engine/src/nodes/fixture/mapping/svg_path/svg_path_fit.rs`
- `lp-core/lpc-engine/src/nodes/fixture/mapping/svg_path/mod.rs`

Suggested resolver API:

```rust
pub fn resolve_svg_path_mapping(
    svg: &str,
    texture_width: u32,
    texture_height: u32,
    sample_diameter: f32,
) -> Result<MappingConfig, SvgPathError>;
```

Resolution behavior:

- Parse SVG mapping groups.
- Sort by `path_index`.
- Reject duplicate path indexes.
- For each group, convert straight-line geometry to a polyline.
- Reject empty/zero-length geometry.
- Sample `count` lamps:
  - `count == 1`: first point.
  - `count > 1`: distances `i / (count - 1)` along total path length.
- Preserve aspect ratio:
  - Use root `viewBox` bounds if Phase 2 parsed them.
  - Otherwise use bounds of all mapping geometry.
  - Fit into `[0, 1] x [0, 1]` according to `texture_width:texture_height`.
  - Center on the padded axis.
- Emit `MappingConfig::PathPoints { paths, sample_diameter }`.
- Each emitted path is `PathSpec::PointList { first_channel, points }`.

`generate_mapping_points` changes:

- Match `PathSpec::PointList`.
- Convert `sample_diameter` to normalized radius the same way as `RingArray`.
- Emit `MappingPoint` for each point in key order.
- Channel is `first_channel + point_index`.
- Clamp centers to `[0, 1]`.

`ProjectLoader` changes:

- Before attaching a fixture node, call a helper such as:

  ```rust
  let mapping = resolve_fixture_mapping(root, &node.source_base_path, config)?;
  ```

- If mapping is `SvgPath`, read the referenced SVG and resolve it.
- Pass the resolved mapping to `FixtureNode::new`.
- Preserve authored artifact payload in the artifact store; do not mutate the stored source def just
  to attach runtime.

Tests:

- Loader resolves a fixture TOML with `SvgPath` and attached SVG from `LpFsMemory`.
- Missing SVG file produces a clear project load error.
- Duplicate `path:N` produces a clear project load error.
- Ungrouped text starting with `path:` produces a clear project load error.
- Curve commands inside mapping groups produce a clear project load error.
- `PointList` generation produces expected channels and normalized positions.
- Aspect-fit test covers wide source into square texture without stretching.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine nodes::fixture::mapping
cargo test -p lpc-engine engine::project_loader
cargo check -p lpc-engine
```
