# Phase 2: Tiny SVG Parser

## Scope Of Phase

Add a constrained SVG mapping parser and geometry flattener in `lpc-engine`.

In scope:

- Parse top-level `<g>...</g>` blocks from SVG text.
- Extract mapping groups that contain one path-like element and one text label.
- Parse labels of the form `path:N,count:N`.
- Parse `<polyline points="...">`.
- Parse basic straight-line `<path d="...">` commands.
- Reject curve commands in mapping groups.
- Reject any text node starting with `path:` unless it is inside a valid mapping group.
- Unit-test the parser with minimal fixtures and the current FYeah-style structure.

Out of scope:

- Loader integration.
- Model changes beyond using types from Phase 1.
- General XML/SVG compliance.
- Nested groups.
- SVG transforms.
- CSS and style interpretation.
- Bezier/curve support.

## Code Organization Reminders

- Put SVG parser code under `lp-core/lpc-engine/src/nodes/fixture/mapping/svg_path/`.
- Prefer one clear concept per file:
  - `svg_path_group.rs` for extracted groups and labels.
  - `svg_path_parser.rs` for scanning SVG groups/attributes/text.
  - `svg_path_data.rs` for path/polyline geometry parsing.
  - `svg_path_error.rs` for parser errors.
- Keep helper functions below public entry points.
- Tests go at the bottom of their files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/fixture/mapping/mod.rs`
- New directory: `lp-core/lpc-engine/src/nodes/fixture/mapping/svg_path/`

Suggested public API:

```rust
pub fn parse_svg_path_groups(svg: &str) -> Result<Vec<SvgPathGroup>, SvgPathError>;
```

Suggested extracted group:

```rust
pub struct SvgPathGroup {
    pub path_index: u32,
    pub count: u32,
    pub geometry: SvgPathGeometry,
}
```

Group extraction rules:

- Scan for `<g` and the next `</g>`.
- Do not recursively scan inside groups.
- If a group body contains `<g`, ignore it unless it also contains a mapping label; then return a
  clear nested-group unsupported error.
- Extract text by finding `<text...>...</text>` and stripping any nested tags such as `<tspan>`.
- Normalize whitespace before parsing the label.
- Parse only `path:N,count:N`; reject malformed labels.
- Before or during group extraction, scan all `<text...>...</text>` payloads in the SVG. Any
  normalized text that starts with `path:` must be accounted for by a valid top-level mapping group.
  If not, return an error that points at ungrouped or invalid mapping text.
- Count path-like elements:
  - `<path ... d="...">`
  - `<polyline ... points="...">`
- Ignore groups without a mapping label.
- Reject groups with a mapping label and zero or multiple path-like elements.

Geometry rules:

- `polyline` supports coordinate lists with spaces and/or commas.
- Basic `path` commands:
  - `M/m`
  - `L/l`
  - `H/h`
  - `V/v`
  - `Z/z` may be ignored or treated as a line back to subpath start only if needed.
- Unsupported commands inside a mapping group return `SvgPathError::UnsupportedCommand`.
- Explicitly unsupported curve commands include `C/c`, `S/s`, `Q/q`, `T/t`, and `A/a`.

Tests:

- Ignores outline/post groups without mapping labels.
- Ignores ungrouped geometry without mapping labels.
- Rejects ungrouped `<text>path:1,count:10</text>`.
- Rejects a group containing `path:` text but no path-like element.
- Parses a group with `<polyline>` and `<text><tspan>path:1,count:10</tspan></text>`.
- Parses a group with `<path d="M0,0 L10,0 H20 V10">`.
- Rejects duplicate path-like elements inside one mapping group.
- Rejects malformed labels.
- Rejects curve commands such as `C` inside mapping groups.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine nodes::fixture::mapping::svg_path
```
