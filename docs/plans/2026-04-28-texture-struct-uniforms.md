# Design

## Scope of Work

Add support for `Texture2D` / `sampler2D` fields inside aggregate uniform
structs so LightPlayer can put regular params and gradient/palette textures in
one `params` uniform struct.

Target use case:

```glsl
struct Params {
    float amount;
    sampler2D gradient;
};

uniform Params params;

vec4 render(vec2 pos) {
    return texture(params.gradient, vec2(pos.x, 0.0)) * params.amount;
}
```

In scope:

- Support texture fields in uniform structs using canonical dotted paths such
  as `params.gradient`.
- Preserve existing top-level texture uniforms such as `inputColor`.
- Reuse existing std430 path machinery (`LpsPathSeg`, `parse_path`,
  `LpsTypePathExt`) internally instead of adding ad hoc string splitting.
- Extend texture binding spec validation to discover nested `Texture2D` fields.
- Extend `LpsPxShader::render_frame` uniform application so nested
  `LpsValueF32::Struct` fields are recursively applied and texture bindings are
  validated with their dotted path.
- Extend frontend texture lowering so `texelFetch(params.gradient, ...)` and
  `texture(params.gradient, ...)` resolve to the correct VMContext descriptor
  offset and spec key.
- Extend filetest texture spec/data directives to accept dotted texture names
  and add positive/negative coverage.
- Update docs and plan notes for the dotted texture path contract.

Out of scope:

- Arrays of textures or indexed texture paths such as `params.gradients[0]`.
- Texture function parameters.
- New texture formats, filters, wraps, shape hints, or sampling behavior.
- Broad GLSL parser work beyond struct fields containing `sampler2D` /
  `texture2D`.
- Product/domain schema changes.

## File Structure

```text
lp-shader/
├── lps-shared/src/
│   ├── path.rs                         # EXISTING: parsed path representation
│   ├── path_resolve.rs                 # UPDATE: path-segment helper entry points if useful
│   └── texture_binding_validate.rs     # UPDATE: recursive Texture2D discovery by dotted path
├── lps-frontend/src/
│   ├── parse.rs                        # UPDATE: narrow sampler2D-in-struct rewrite if feasible
│   ├── lower_texture.rs                # UPDATE: resolve params.gradient texture operands
│   └── sampler2d_metadata_tests.rs     # UPDATE: nested texture metadata/lowering tests
├── lp-shader/src/
│   ├── px_shader.rs                    # UPDATE: recursive nested uniform application
│   └── tests.rs                        # UPDATE: public render_frame test with params.gradient
└── lps-filetests/
    ├── src/parse/parse_texture.rs      # UPDATE: dotted texture directive validation/tests
    └── filetests/texture/              # NEW: positive + negative struct texture filetests

docs/
├── design/lp-shader-texture-access.md  # UPDATE: dotted texture path contract
└── plans/
    └── 2026-04-28-texture-struct-uniforms.md
```

## Conceptual Architecture

```text
GLSL params.gradient
        │
        ▼
frontend metadata
  uniforms_type: Struct { params: Struct { gradient: Texture2D, ... } }
        │
        ▼
canonical texture path
  "params.gradient"
        │
        ├─ TextureBindingSpec lookup
        ├─ runtime texture validation
        ├─ lpvm set_uniform("params.gradient", Texture2D)
        └─ filetest texture-spec / texture-data names
```

## Main Components

### Canonical Texture Paths

Public and filetest interfaces continue to use `String` keys because that is
what `TextureBindingSpecs`, `CompilePxDesc`, and filetest directives already
use. Nested texture fields use canonical dotted paths, e.g.
`params.gradient`.

Implementation code should not manually split on `.`. Use the existing parsed
path machinery:

- `lps-shared/src/path.rs`: `LpsPathSeg`, `parse_path`.
- `lps-shared/src/path_resolve.rs`: `LpsTypePathExt::type_at_path` and
  `offset_for_path`.

Add helper APIs around these if a phase needs to traverse already-parsed
segments or enumerate texture leaves; do not create a competing path parser.

### Shared Validation

`validate_texture_binding_specs_against_module` currently checks only top-level
`Texture2D` members. It should discover `Texture2D` leaves recursively and
compare the resulting dotted path set against `TextureBindingSpec` keys.

Existing top-level keys such as `inputColor` must continue to validate.
Indexed texture paths are out of scope for this first pass; if discovery sees a
texture inside an array, return a clear unsupported error or skip support with a
clear diagnostic rather than inventing `foo[0]` binding semantics.

### Runtime Binding

`lpvm::set_uniform(path, value)` already resolves string paths and writes the
leaf type at the computed offset. `LpsPxShader::apply_uniforms` needs to bridge
public nested `LpsValueF32::Struct` inputs into that path API:

- recursively walk expected `LpsType::Struct` members and actual
  `LpsValueF32::Struct` fields;
- build canonical dotted paths;
- for `Texture2D` leaves, validate the `LpsTexture2DValue` against
  `meta.texture_specs[path]`;
- call `inner.set_uniform(path, value)` for each leaf.

This should preserve current flat top-level behavior and error messages should
include the full dotted path.

### Frontend Lowering

`lower_texture.rs` currently requires sampled image operands to be direct
`Expression::GlobalVariable` handles. Struct-field operands such as
`params.gradient` are expected to arrive as a Naga access chain. The lowering
needs a resolver that can:

- identify the root uniform global;
- collect struct field names/indexes to construct a canonical texture path;
- compute the descriptor byte offset for that path using the same std430 layout
  rules as the rest of the uniform path machinery;
- look up `TextureBindingSpec` by the dotted path;
- reject texture arrays/indexed paths clearly.

The first implementation should prefer a narrow resolver local to texture
lowering unless an existing aggregate-access helper can be reused cleanly.

Parser note: top-level `uniform sampler2D x;` is currently rewritten in
`parse.rs` because Naga does not parse `sampler2D` as a type there. Phase 3
should spike whether a narrow rewrite for struct fields is feasible. If it
requires a broad parser rewrite, stop and report; do not silently expand scope.

### Filetests

Texture filetests must cover this feature, using dotted directive names:

```glsl
// texture-spec: params.gradient format=rgba16unorm filter=nearest wrap=clamp shape=height-one
// texture-data: params.gradient 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0
```

Add positive and negative filetests for struct texture fields. Keep tests small
and aligned with the existing `lp-shader/lps-filetests/filetests/texture/`
style.

# Phases

## Phase 1: Shared Path Discovery And Spec Validation

[sub-agent: yes]

### Scope of Phase

Teach shared texture binding validation to discover nested `Texture2D` fields
in uniform structs and validate them using canonical dotted paths.

In scope:

- Add a recursive texture-leaf discovery helper in `lps-shared`.
- Use existing `LpsPathSeg`, `parse_path`, and/or `LpsTypePathExt` internally
  where path traversal or validation is needed.
- Update `validate_texture_binding_specs_against_module` to accept:
  - top-level keys like `inputColor`;
  - nested keys like `params.gradient`.
- Add unit tests for matching, missing, and extra nested specs.
- Add a clear unsupported diagnostic for texture arrays/indexed texture paths if
  the type walker encounters them.

Out of scope:

- Runtime uniform application.
- Frontend lowering.
- Filetest directive parsing.
- Public API shape changes to `TextureBindingSpecs`.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Relevant files:

- `lp-shader/lps-shared/src/texture_binding_validate.rs`
- `lp-shader/lps-shared/src/path.rs`
- `lp-shader/lps-shared/src/path_resolve.rs`
- `lp-shader/lps-shared/src/layout.rs`
- `lp-shader/lps-shared/src/lib.rs` if a helper needs exporting

Suggested internal shape:

```rust
fn declared_texture2d_paths(meta: &LpsModuleSig) -> Result<BTreeSet<String>, String>;

fn collect_texture2d_paths(
    ty: &LpsType,
    prefix: &mut Vec<String>,
    out: &mut BTreeSet<String>,
) -> Result<(), String>;
```

Keep public keys as `String`. For path validation, parse returned strings with
`parse_path` in tests or helpers if useful, but do not require callers to
construct `Vec<LpsPathSeg>`.

Test cases:

- top-level `inputColor: Texture2D` still requires `inputColor`;
- nested `params.gradient: Texture2D` requires `params.gradient`;
- missing nested spec errors with `params.gradient`;
- extra nested spec errors with the extra key;
- nested non-texture fields are ignored;
- texture inside array is rejected or documented as unsupported with a clear
  message.

### Validate

Run:

```bash
cargo test -p lps-shared texture_binding
cargo test -p lps-shared path
```

## Phase 2: Runtime Recursive Uniform Application

[sub-agent: yes]

### Scope of Phase

Update `LpsPxShader::render_frame` uniform application to recursively apply
nested uniform structs, validate nested texture values with dotted spec keys,
and call `set_uniform` with dotted paths.

In scope:

- Refactor `LpsPxShader::apply_uniforms` in `lp-shader/src/px_shader.rs`.
- Add helper functions for recursive expected-type/value matching.
- Validate nested `Texture2D` values with `validate_runtime_texture_binding`.
- Add public `lp-shader` tests for nested texture binding through
  `render_frame`.
- Preserve existing flat top-level uniform behavior and errors.

Out of scope:

- Frontend lowering for sampling from nested textures.
- Filetest support.
- New public rendering API.
- Texture arrays.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Relevant files:

- `lp-shader/lp-shader/src/px_shader.rs`
- `lp-shader/lp-shader/src/tests.rs`
- `lp-shader/lp-shader/src/runtime_texture_validation.rs`
- `lp-shader/lps-shared/src/lps_value_f32.rs`

Suggested recursive behavior:

```rust
apply_uniform_value(
    inner,
    expected_ty,
    value,
    path_prefix,
    texture_specs,
)
```

Rules:

- At a struct node, require `LpsValueF32::Struct`.
- For each expected named member, find a matching actual field by name.
- Build a dotted path by appending the member name to the current prefix.
- At `Texture2D`, require `LpsValueF32::Texture2D`, look up
  `meta.texture_specs[path]`, validate, then call `set_uniform(path, value)`.
- At non-struct leaf values, call `set_uniform(path, value)`.
- Error messages should include `params.gradient`, not only `gradient`.

Tests in `lp-shader/lp-shader/src/tests.rs`:

- construct a fake or compiled shader metadata shape with
  `params.gradient: Texture2D` and verify a nested `LpsValueF32::Struct` applies
  successfully;
- missing `params.gradient` reports the full path;
- wrong value type for `params.gradient` reports `Texture2D` and full path;
- runtime validation failure for `HeightOne` nested texture reports the full
  path;
- existing top-level texture tests still pass unchanged.

If a full render test cannot compile until Phase 3 frontend work lands, use the
smallest existing `LpsPxShader` construction path available in tests and leave
full render coverage to Phase 4. Do not add stubs.

### Validate

Run:

```bash
cargo test -p lp-shader texture
cargo test -p lpvm set_uniform
```

## Phase 3: Frontend Lowering For Struct Texture Operands

[sub-agent: supervised]

### Scope of Phase

Teach frontend texture lowering to resolve `params.gradient` texture operands
for both `texelFetch` and filtered `texture()`.

In scope:

- Add a resolver in `lps-frontend/src/lower_texture.rs` for direct texture
  globals and struct-field texture access chains.
- Construct canonical dotted texture paths for spec lookup and diagnostics.
- Compute descriptor byte offsets for nested texture fields using existing
  std430 path/layout machinery.
- Keep top-level direct texture uniforms working unchanged.
- Add frontend tests for metadata/lowering of nested texture fields.
- Spike narrow `sampler2D` struct-field parsing support in `parse.rs` if Naga
  needs it.

Out of scope:

- Arrays of textures.
- Texture function parameters.
- Broad GLSL parser rewrite.
- New sampling builtins or formats.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If the `sampler2D` struct-field rewrite becomes broad or ambiguous, stop and
  report before implementing it.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Relevant files:

- `lp-shader/lps-frontend/src/lower_texture.rs`
- `lp-shader/lps-frontend/src/lower_expr.rs`
- `lp-shader/lps-frontend/src/lower.rs`
- `lp-shader/lps-frontend/src/parse.rs`
- `lp-shader/lps-frontend/src/sampler2d_metadata_tests.rs`
- `lp-shader/lps-shared/src/path_resolve.rs`

Current blocker:

- `resolve_direct_texture2d_uniform` only accepts
  `Expression::GlobalVariable` and returns `(GlobalVariable, name)`.

New resolver should return something like:

```rust
struct TextureOperand {
    path: String,
    descriptor_base_byte_offset: u32,
    ty: LpsType,
}
```

It should support:

- top-level `inputColor`;
- struct-field `params.gradient`.

The descriptor loading code should use the resolved descriptor base byte offset
instead of assuming the direct global's `GlobalVarInfo::byte_offset`.

For `texture()` calls, also ensure `parse.rs` rewrites calls if needed. The
preferred user GLSL surface is `texture(params.gradient, uv)`, not requiring
users to hand-write `sampler2D(...)`. If Naga cannot parse `sampler2D` fields
in structs without substantial parser work, stop and report with the exact Naga
shape/error.

Tests:

- `texelFetch(params.gradient, ivec2(0, 0), 0)` lowers with spec key
  `params.gradient`;
- `texture(params.gradient, vec2(...))` lowers to the expected 2D or height-one
  builtin;
- missing spec for `params.gradient` errors with the full dotted path;
- top-level texture lowering tests continue to pass.

### Validate

Run:

```bash
cargo test -p lps-frontend texture
cargo test -p lps-frontend sampler2d
cargo test -p lps-shared texture_binding
```

## Phase 4: Filetest Directive And Behavior Coverage

[sub-agent: yes]

### Scope of Phase

Add GLSL filetest coverage for texture fields inside uniform structs using
dotted `texture-spec` and `texture-data` names.

In scope:

- Update filetest parser validation if needed so dotted texture names are
  parsed and validated as paths.
- Add positive filetests for:
  - `texelFetch(params.gradient, ...)`;
  - `texture(params.gradient, ...)` with `HeightOne` gradient/palette-style
    sampling.
- Add negative filetests for:
  - missing nested spec;
  - extra nested spec;
  - nested runtime fixture mismatch or missing fixture.
- Keep filetests small and under `lp-shader/lps-filetests/filetests/texture/`.

Out of scope:

- New directive syntax beyond dotted names.
- Texture arrays/indexed directive names.
- Broad filetest harness refactors.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by frontend parsing or lowering behavior from Phase 3, stop and
  report rather than marking tests unsupported.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Relevant files:

- `lp-shader/lps-filetests/src/parse/parse_texture.rs`
- `lp-shader/lps-filetests/src/test_run/texture_fixture.rs`
- `lp-shader/lps-filetests/filetests/texture/`
- `lp-shader/lps-filetests/README.md`

Examples to add:

```glsl
// test run
// texture-spec: params.gradient format=rgba16unorm filter=nearest wrap=clamp shape=height-one
// texture-data: params.gradient 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0

struct Params {
    float amount;
    sampler2D gradient;
};

uniform Params params;

vec4 sample_gradient() {
    return texture(params.gradient, vec2(0.75, 0.5)) * params.amount;
}
```

If the parser uses `texture2D` internally after Phase 3, keep the public
filetest shader in the preferred user surface when possible. Only use explicit
`texture2D` if Phase 3 documented a temporary frontend limitation.

Run texture filetests for all supported texture targets, not only Rust parser
tests.

### Validate

Run:

```bash
cargo test -p lps-filetests texture
scripts/filetests.sh --target wasm.q32,rv32n.q32,rv32c.q32 texture/
```

## Phase 5: Cleanup, Docs, And Validation

[sub-agent: supervised]

### Scope of Phase

Clean up the implementation, document the dotted texture path contract, and run
the final validation suite.

In scope:

- Update `docs/design/lp-shader-texture-access.md` for texture fields inside
  uniform structs and dotted spec keys.
- Update `lp-shader/lps-filetests/README.md` if Phase 4 changed directive
  behavior.
- Move remaining planning notes to the bottom of this file under `# Notes`.
- Append `# Decisions for future reference`.
- Run final validation commands.

Out of scope:

- New features beyond the completed phases.
- Product/domain schema work.
- Archiving this file manually; standalone plan archiving happens at commit
  time if the implementation process requests it.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If validation fails for a non-obvious reason, stop and report rather than
  debugging deeply.
- Report back: what changed, what was validated, and any deviations.

### Implementation Details

Review the diff for:

- ad hoc dotted-string parsing instead of `parse_path` / `LpsPathSeg` helpers;
- accidentally broken top-level texture uniforms;
- texture arrays or indexed texture paths accidentally partially supported;
- new `#[allow(...)]`, `todo!`, `unimplemented!`, `dbg!`, debug prints, or
  stale temporary comments;
- tests weakened or marked unsupported instead of fixed.

Append decisions worth keeping, likely:

- public texture binding keys remain canonical strings;
- implementation uses parsed path helpers internally;
- first pass supports struct-field textures only, not texture arrays.

### Validate

Run:

```bash
cargo test -p lps-shared texture_binding
cargo test -p lp-shader texture
cargo test -p lps-frontend texture
cargo test -p lps-filetests texture
scripts/filetests.sh --target wasm.q32,rv32n.q32,rv32c.q32 texture/
just check
```

# Notes

## Current State (implementation)

Phases 1–4 landed: nested uniform struct `sampler2D` fields use canonical dotted
keys (`params.gradient`) for `TextureBindingSpec`, recursive runtime uniform
application, frontend texture lowering for access-chain operands, and filetests
with dotted `texture-spec` / `texture-data` names validated via `parse_path`
(no indexed directive segments).

Shared infrastructure reused as planned:

- `lps-shared/src/path.rs` — `LpsPathSeg`, `parse_path`
- `lps-shared/src/path_resolve.rs` — `type_at_path` / `offset_for_path` for
  std430 layout
- Guest uniform writes still go through `lpvm::set_uniform(path, …)` string paths.

## Resolved Answers

- Public texture spec keys remain canonical strings such as `params.gradient`.
- Implementation internals should reuse `LpsPathSeg`, `parse_path`, and
  `LpsTypePathExt` rather than adding ad hoc string splitting.
- Initial support is limited to struct-field textures; arrays of textures and
  indexed texture paths are out of scope.
- Existing top-level texture uniforms remain compatible.
- Filetests should use dotted directive names, e.g.
  `// texture-spec: params.gradient ...`, and must cover this functionality.
- Prefer GLSL `sampler2D` fields in structs if feasible with a narrow frontend
  rewrite; if that requires a much larger parser change, stop and report before
  implementation.

# Decisions for future reference

- **Public API keys:** Texture binding specs, compile descriptors, runtime
  uniform paths, and filetest directives all use canonical string keys (either a
  single identifier or a dotted path such as `params.gradient`).
- **Internals:** Path parsing and validation use `parse_path` / `LpsPathSeg` and
  `LpsTypePathExt`; avoid splitting dotted strings ad hoc elsewhere.
- **Scope:** Nested struct fields only for the first shipment; texture arrays and
  indexed sampler paths (`params.textures[0]`) remain unsupported until
  separately designed.

