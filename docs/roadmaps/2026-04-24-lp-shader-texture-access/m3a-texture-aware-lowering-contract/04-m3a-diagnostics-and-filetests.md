# Phase 4: M3a Diagnostics and Filetests

## Scope of phase

Add user-facing filetest coverage for the M3a texture-aware lowering contract.

In scope:

- Add texture lowering diagnostic filetests for `texelFetch` recognition and rejection cases.
- Ensure filetests pass texture specs into frontend lowering through the Phase 2 wiring.
- Keep M3a tests focused on compile/lower diagnostics, not successful sampling values.
- Update any M2 placeholder texture fixture example only if it currently conflicts with M3a diagnostics.

Out of scope:

- Do not add exact-value `texelFetch` runtime tests; those belong to M3b/M3c.
- Do not implement data-path codegen to make filetests pass.
- Do not broaden fixture syntax.
- Do not change parser directive behavior except for small test expectation updates.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If expected diagnostics are unclear because Phase 3 behaved differently than planned, stop and report.
- Report back what changed, what was validated, and any deviations from this phase plan.

## Implementation Details

Read these files first:

- `lp-shader/lps-filetests/filetests/textures/`
- `lp-shader/lps-filetests/src/test_error/mod.rs`
- `lp-shader/lps-filetests/src/test_run/run_detail.rs`
- `lp-shader/lps-filetests/src/parse/test_type.rs`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/00-design.md`

### Filetest style

Use the existing `.glsl` comment directive style. For lower/compile diagnostics, prefer `// test glsl-error` with `// EXPECT_ERROR:` lines if that is the current convention in adjacent texture filetests.

Do not add runtime `// run:` exact value assertions in this phase.

### Suggested filetests

Add files under `lp-shader/lps-filetests/filetests/textures/` with names like:

- `error_texelfetch_m3b_placeholder.glsl`
- `error_texelfetch_nonzero_lod.glsl`
- `error_texelfetch_dynamic_lod.glsl`
- `error_texelfetch_missing_spec.glsl`

Use concise shaders:

```glsl
// test glsl-error
// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// EXPECT_ERROR: {{texelFetch}}
// EXPECT_ERROR: {{M3b}}

uniform sampler2D inputColor;

vec4 render(vec2 pos) {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}
```

For nonzero LOD:

```glsl
return texelFetch(inputColor, ivec2(0, 0), 1);
```

For dynamic LOD:

```glsl
uniform int lod;
return texelFetch(inputColor, ivec2(0, 0), lod);
```

For missing spec, omit `// texture-spec:` and assert the sampler name appears.

Exact directive names and expectation syntax should match existing filetests. If current diagnostics are line-numbered or use different error codes, follow existing style rather than inventing a new one.

### Existing placeholder fixture

Review `positive_minimal_fixture_design_doc.glsl`. If it still states that `texelFetch` is unsupported in a way that conflicts with M3a, update comments to say M3a recognizes the contract but full data-path codegen is M3b. Do not convert it into a passing runtime fetch test in this phase.

## Validate

Run targeted texture filetests if the harness supports a file/path filter. If not, run:

```bash
cargo test -p lps-filetests --test filetests
```

Also run:

```bash
cargo test -p lps-frontend
```

Report exact commands and results.

