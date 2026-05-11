# Phase 2 — Texture Directive Parser Model

## Scope of Phase

Add parser model support for file-level texture specs and inline texture
fixture declarations in `lps-filetests`.

Out of scope:

- Do not allocate backend shared memory.
- Do not bind texture uniforms at runtime.
- Do not add texture sampling execution behavior.
- Do not add the full diagnostic filetest suite yet, except parser unit tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Read first:

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m2-texture-filetests/00-design.md`
- `lp-shader/lps-filetests/src/parse/mod.rs`
- `lp-shader/lps-filetests/src/parse/test_type.rs`
- `lp-shader/lps-filetests/src/parse/parse_set_uniform.rs`
- `lp-shader/lps-shared/src/texture_format.rs`

Add a new parser module:

- `lp-shader/lps-filetests/src/parse/parse_texture.rs`

Update parser exports:

- `lp-shader/lps-filetests/src/parse/mod.rs`
- `lp-shader/lps-filetests/src/parse/test_type.rs`

Suggested data model in `test_type.rs`:

```rust
pub type TextureSpecs = std::collections::BTreeMap<String, TextureBindingSpec>;
pub type TextureFixtures = std::collections::BTreeMap<String, TextureFixture>;

#[derive(Debug, Clone)]
pub struct TextureFixture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: TextureStorageFormat,
    pub pixels: Vec<TextureFixturePixel>,
    pub line_number: usize,
}

#[derive(Debug, Clone)]
pub struct TextureFixturePixel {
    pub channels: Vec<TextureFixtureChannel>,
}

#[derive(Debug, Clone)]
pub enum TextureFixtureChannel {
    NormalizedFloat(f32),
    ExactHex(u16),
}
```

It is fine to adjust names if a clearer local pattern exists, but keep the
concepts file-level and backend-neutral.

`TestFile` should gain:

```rust
pub texture_specs: BTreeMap<String, TextureBindingSpec>,
pub texture_fixtures: BTreeMap<String, TextureFixture>,
```

Directive grammar:

```glsl
// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0
```

Parsing rules:

- `texture-spec` is one line.
- Required keys: `format`, `filter`, `shape`, and either `wrap` or both axes
  if axis-specific keys are used.
- Accepted formats: `r16unorm`, `rgb16unorm`, `rgba16unorm`.
- Accepted filters: `nearest`, `linear`.
- Accepted wrap spellings:
  - `clamp` or `clamp-to-edge` -> `TextureWrap::ClampToEdge`
  - `repeat` -> `TextureWrap::Repeat`
  - `mirror-repeat` or `mirror_repeat` -> `TextureWrap::MirrorRepeat`
- `wrap=<mode>` sets both axes.
- If cheap, support `wrap_x=<mode>` and `wrap_y=<mode>`; if one axis-specific
  key appears without the other and no `wrap=` fallback exists, emit a parse
  error.
- Accepted shapes:
  - `2d` -> `TextureShapeHint::General2D`
  - `height-one` or `height_one` -> `TextureShapeHint::HeightOne`

`texture-data` parsing rules:

- Header line is `// texture-data: <name> <width>x<height> <format>`.
- Pixel data lines immediately after the header are comment lines whose trimmed
  body begins with `//`.
- Pixel data ends at the next recognized filetest directive, blank comment
  separator, GLSL line, or EOF.
- Pixel tokens are whitespace-separated.
- Channel tokens inside a pixel are comma-separated with no spaces.
- Channel values are either normalized floats (`0`, `0.0`, `1.0`, etc.) or
  exact hex storage values.
- For M2 formats, exact hex channels are 4 hex digits per unorm16 channel,
  case-insensitive.

Be careful in `parse/mod.rs`:

- Continue using `strip_block_comment_fragments` before detecting texture
  directives.
- Do not mistake normal GLSL comments for pixel rows unless currently inside a
  `texture-data` block.
- Detect duplicate texture specs or duplicate fixtures by name and return a
  clear line-aware parse error.

Unit tests:

- Parse a minimal file with one spec and one fixture.
- Parse `wrap=clamp` into both axes.
- Parse `wrap_x` / `wrap_y` if implemented.
- Parse `shape=height-one` and `shape=height_one`.
- Reject duplicate specs.
- Reject duplicate fixtures.
- Reject unsupported filter/wrap/shape spellings.
- Reject malformed `texture-data` header.
- Ensure directives inside block comments are ignored, matching existing
  directive behavior.

## Validate

Run from repo root:

```bash
cargo test -p lps-filetests parse_texture
cargo test -p lps-filetests parse
```

