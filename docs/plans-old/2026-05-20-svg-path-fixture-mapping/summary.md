# SVG Path Fixture Mapping Summary

## What was built

- Added an authored `SvgPath` fixture mapping variant with a source SVG path and sample diameter.
- Added runtime `PointList` path specs so resolved SVG lamps can reuse the existing fixture mapping and precompute pipeline.
- Added a strict, tiny SVG mapping resolver for top-level groups containing exactly one path-like element and exactly one `path:N,count:N` text node.
- Supported straight-line geometry only: `polyline` plus `M/m`, `L/l`, `H/h`, `V/v`, and `Z/z` path commands.
- Rejected ungrouped `path:` text, nested mapping groups, duplicate path indexes, curves, missing paths, multiple path-like elements, malformed labels, and zero counts.
- Wired project loading so fixture TOML `SvgPath` mappings resolve relative to the fixture file and become `PathPoints` before `FixtureNode` construction.
- Updated the checked-in button sign example to use a cleaned SVG mapping file.
- Added parser, model round-trip, project loader, and example validation coverage.

## Decisions for future reference

#### Loader-time resolution

- **Decision:** Resolve `SvgPath` mappings in `ProjectLoader` and pass `FixtureNode` a resolved `PathPoints` mapping.
- **Why:** The render/precompute path stays simple and does not parse text SVG content at runtime.
- **Rejected alternatives:** Carry unresolved SVG references into fixture runtime; parse SVG in the model crate.
- **Revisit when:** A permanent fixture/layout authoring system replaces this temporary bridge.

#### Tiny strict parser

- **Decision:** Use a small string parser for the constrained SVG subset instead of adding an SVG/XML dependency.
- **Why:** The current format is deliberately narrow and authored for this project; strict errors catch Illustrator grouping mistakes.
- **Rejected alternatives:** General SVG parsing, transform support, nested traversal, and broad XML correctness.
- **Revisit when:** Mapping authoring expands beyond cleaned, top-level straight-line SVG groups.

#### Straight lines only

- **Decision:** Support `polyline` and straight path commands, and reject curve commands.
- **Why:** The current FYeah mapping has been constrained to straight lines, and curve support would add parser and sampling complexity.
- **Rejected alternatives:** Cubic/quadratic/arc flattening in the MVP.
- **Revisit when:** Authored mappings need real curved segments.

#### Path labels as ordering keys

- **Decision:** Treat `path:N` as a path ordering key, not a direct channel offset.
- **Why:** Sorting by path and accumulating `count` matches the end-to-end strip assumption while keeping channel assignment deterministic.
- **Rejected alternatives:** Interpreting `path:N` as the first channel index.
- **Revisit when:** Fixture mappings need non-contiguous or explicitly addressed channel ranges.
