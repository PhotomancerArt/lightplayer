# Phase 1: Model Contract

## Scope Of Phase

Add the authored and runtime model shapes needed for SVG path fixture mapping.

In scope:

- Extend fixture mapping model types.
- Add constructors/helpers for tests and examples.
- Add TOML parse/write tests for the new shape.

Out of scope:

- SVG parsing.
- Project loader expansion.
- Example project changes.
- Runtime point generation changes beyond compile fallout from new enum variants.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep fixture mapping domain types in `lp-core/lpc-model/src/nodes/fixture/mapping.rs` unless the
  file becomes unwieldy.
- Put tests at the bottom of the file.
- Do not introduce broad dependencies.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/fixture/mapping.rs`
- `lp-core/lpc-model/src/nodes/fixture/mod.rs`
- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/src/nodes/node_def.rs`

Expected model changes:

- Add `MappingConfig::SvgPath`.
- Suggested fields:

  ```rust
  SvgPath {
      source: SourcePathSlot,
      sample_diameter: PositiveF32Slot,
  }
  ```

- Add `PathSpec::PointList`.
- Suggested fields:

  ```rust
  PointList {
      first_channel: ValueSlot<u32>,
      points: MapSlot<u32, XySlot>,
  }
  ```

- Add constructors:
  - `MappingConfig::svg_path(source: impl Into<SourcePath>, sample_diameter: f32)`
  - `PathSpec::point_list(first_channel: u32, points: impl IntoIterator<Item = [f32; 2]>)`

TOML shape should accept:

```toml
[mapping]
kind = "SvgPath"
source = "./fyeah-mapping.svg"
sample_diameter = 2.0
```

The existing slot codec may serialize enum variants with Rust-case names such as `SvgPath`.
If the local convention strongly prefers snake case, record the codec limitation rather than
rewriting the enum codec in this phase.

Tests:

- `NodeDef::read_toml` parses a fixture containing `SvgPath`.
- `NodeDef::write_toml` round-trips a fixture containing `SvgPath`.
- `PathSpec::PointList` round-trips through fixture TOML or a direct registry test.

Compile fallout:

- Exhaustive matches in `lpc-engine` will need temporary branches for new variants. For this phase,
  those branches may return empty points for `SvgPath` and handle `PointList` in the next phase.
  Prefer adding the minimal code needed to keep builds green.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model nodes::node_def
cargo check -p lpc-engine
```
