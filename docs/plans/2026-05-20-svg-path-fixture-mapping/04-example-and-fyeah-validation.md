# Phase 4: Example And FYeah Validation

## Scope Of Phase

Add or update an example project that exercises the `SvgPath` fixture mapping with an SVG shaped
like the FYeah mapping file.

In scope:

- Add a cleaned SVG fixture mapping file to an example.
- Reference it from fixture TOML with `MappingConfig::SvgPath`.
- Keep the example small enough for normal example validation.
- Validate that the loader extracts the expected path/count data.

Out of scope:

- Physical hardware validation.
- Completing all letter paths in the real sign art.
- Replacing the broader example/radio/playlist flow.
- UI editing support for SVG mappings.

## Code Organization Reminders

- Prefer extending `examples/button-sign` if that is still the FYeah-adjacent example.
- If creating `examples/fyeah-sign`, keep duplicated shaders minimal.
- Keep the SVG source file checked in only if it is intentionally small and cleaned for the parser
  subset.
- Avoid unrelated example churn.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `examples/button-sign/fixture.toml` or a new `examples/fyeah-sign/fixture.toml`
- `examples/button-sign/fyeah-mapping.svg` or `examples/fyeah-sign/fyeah-mapping.svg`
- `lp-cli/tests/examples_valid.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`

Fixture TOML shape:

```toml
[mapping]
kind = "SvgPath"
source = "./fyeah-mapping.svg"
sample_diameter = 2.0
```

Cleaned SVG expectations:

- Root `<svg>` may keep `viewBox`.
- Mapping groups look like:

  ```xml
  <g>
    <path d="M0,0 L10,0 L10,10"/>
    <text><tspan>path:1,count:10</tspan></text>
  </g>
  ```

  or:

  ```xml
  <g>
    <polyline points="0 0 10 0 10 10"/>
    <text>path:2,count:12</text>
  </g>
  ```

- Non-mapping groups may remain; parser should ignore them.
- Ungrouped geometry may remain; parser should ignore it.
- Ungrouped text starting with `path:` must not remain; the parser should reject it to catch missing
  SVG groups.
- Avoid transforms and nested groups in the cleaned example.
- Avoid curve commands in mapping groups; convert them to straight segments in the SVG authoring
  tool before checking in the example.

Tests:

- Extend example validation so the selected example loads through `ProjectLoader::load_from_root`.
- Add a focused assertion that the resolved mapping contains the expected total lamp count for the
  checked-in SVG.
- If a direct runtime inspection API is awkward, test the resolver directly with the checked-in SVG
  contents.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli --test examples_valid
cargo test -p lpc-engine engine::project_loader
cargo test -p lpc-engine nodes::fixture::mapping::svg_path
```
