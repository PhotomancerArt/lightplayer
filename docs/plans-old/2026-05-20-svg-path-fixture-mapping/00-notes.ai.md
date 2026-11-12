# SVG Path Fixture Mapping Notes

## Scope Of Work

Add a deliberately small `svg_path` fixture mapping authoring path for the first FYeah sign project.
This is a temporary, practical bridge from hand-authored SVG centerlines to the existing fixture
mapping/runtime machinery.

In scope:

- Add an authored fixture mapping variant similar to:

  ```toml
  [mapping]
  kind = "SvgPath"
  source = "./fyeah-mapping.svg"
  sample_diameter = 2.0
  ```

- Parse a constrained SVG subset optimized for the current mapping file style.
- Look only for non-nested `<g>...</g>` blocks.
- A mapping group is valid only when it contains exactly one path-like element and one text label.
- The text label format is `path:N,count:N`.
- Sort accepted groups by `path`, then assign channels end-to-end by `count`.
- Ignore non-mapping SVG content, including outline groups, post markers, styles, defs, and
  ungrouped geometry.
- Be strict about mapping-looking text: any `<text>` payload whose normalized content starts with
  `path:` must be inside a valid mapping group.
- Preserve aspect ratio when fitting SVG coordinates into fixture texture coordinates.
- Use the FYeah mapping SVG as validation input after cleaning/copying it into an example fixture.

Out of scope:

- General SVG support.
- Nested group traversal.
- CSS/style interpretation.
- SVG transform support.
- Bezier/curve path commands in the MVP parser.
- Arbitrary XML parsing correctness.
- A permanent fixture/layout authoring system.
- Host-side precompilation or anything that weakens the on-device shader compiler path.

## Current Codebase State

Fixture model shape lives in:

- `lp-core/lpc-model/src/nodes/fixture/mapping.rs`
- `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`

Current authored mappings support:

- `MappingConfig::Unset`
- `MappingConfig::PathPoints { paths, sample_diameter }`
- `PathSpec::RingArray { ... }`

Runtime mapping point generation lives in:

- `lp-core/lpc-engine/src/nodes/fixture/mapping/points.rs`
- `lp-core/lpc-engine/src/nodes/fixture/mapping/precompute.rs`

`generate_mapping_points` currently walks `PathPoints.paths` in map order and assigns channels by a
running `channel_offset`. That matches the desired end-to-end behavior after an SVG mapping is
resolved into ordered path specs.

Project loading lives in:

- `lp-core/lpc-engine/src/engine/project_loader.rs`

The loader already knows how to resolve source files relative to a node artifact through
`resolve_path_relative_to_file` and `read_utf8_file` for shader sources. The SVG mapping resolver
should reuse this pattern for fixture mappings.

Example validation exists in:

- `lp-cli/tests/examples_valid.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs` tests for example projects

Current examples use authored fixture TOML under `examples/*/fixture.toml`.

## FYeah SVG Observations

The referenced file is:

- `/Users/yona/Dropbox/maker-projects/2026-04-28-sign/projects/fyeah/fyeah-mapping.svg`

The current file includes:

- `viewBox="0 0 2146.8 453.5"`.
- Many non-mapping elements: outline, accepted/candidate posts, style definitions, etc.
- Mapping-like `<g>` blocks containing one `polyline` or one `path` plus a `text` label such as
  `path:1,count:10`.
- Some stray ungrouped `path` and `text` elements. Ungrouped geometry can be ignored, but any
  ungrouped text starting with `path:` should fail validation so missing Illustrator groups are
  caught.
- Some path data with Bezier-like commands exists in earlier exports. The current need is constrained
  to straight lines only; Bezier/curve commands should be rejected inside mapping groups.

## User Notes

- This is plan time only; no implementation changes should be made yet.
- The mapping type should be `svg_path` or similar.
- The format should be similar to the current SVG file, but cleaned up.
- Look for `<g>` with a text and a single path.
- Text should follow `path:N,count:N`.
- Later support can include richer fixture/layout authoring.
- Ignore everything else for now.
- Any text node that starts with `path:` must be in a valid group.
- Only straight-line paths need to be supported for now.
- Scale to fit in square or arbitrary texture size without stretching.
- A tiny parser optimized only for this subset is preferred.
- Do not support nested groups or broad XML/SVG features.
- Use the example as validation.
- This probably is not permanent.

## Open Questions

### Should `path:N` be the first channel index or the ordering key?

Context: The old note used `<idx>:<count>`, but the new requested text format is
`path:N,count:N`. The current SVG examples use `path:1`, `path:2`, etc., which reads more like path
order than channel offset.

Suggested answer: Treat `path` as an ordering key, not a channel index. Sort groups by ascending
`path`; assign channels end-to-end from zero by accumulating `count`.

### Should the MVP support `<polyline>` as well as `<path>`?

Context: The current mapping SVG has valid-looking grouped `polyline` elements. The user said "a
single path", but the actual Illustrator output includes polylines for some letter strokes.

Suggested answer: Support one path-like element: either `<path d="...">` or
`<polyline points="...">`. Keep the authored TOML kind named `SvgPath`.

### Should Beziers be implemented immediately?

Context: The current need has been constrained to straight lines only. Supporting cubic/quadratic
curves would expand the parser and make this temporary bridge less strict.

Answer: Do not implement Beziers now. Reject curve commands inside mapping groups with an explicit
error and leave curve support as future work.

### Should ungrouped mapping labels be ignored?

Context: It is easy to forget to group a path/text pair in Illustrator. If ungrouped `path:` text is
ignored, the fixture silently drops lamps.

Answer: No. Scan all text payloads. Any text whose normalized content starts with `path:` must belong
to a valid top-level mapping group; otherwise return a validation error.

### Where should SVG parsing live?

Context: The model crate is `no_std` and should stay mostly authored shape definitions. Loader code
already reads source files and is a natural place to expand authored sources.

Suggested answer: Put parsing/resolution in `lpc-engine` under fixture mapping modules, not in
`lpc-model`. The model only carries the authored `SvgPath` variant and any compact runtime path spec
needed after resolution.
