---
status: fixed
found: 2026-08-05      # how: report — palette shaders never previewed in browser Studio
fixed: this change
area: lp-shader/lps-frontend (parse.rs sampler2D rewrite), lpc-model shader_header_gen
class: config-masked-defect
related:
  - docs/defects/2026-08-01-xtlpn-f32-loses-writes-to-value-parameters.md
  - docs/design/lp-shader-texture-access.md
  - docs/adr/2026-08-04-palettes-are-values.md
---
# The engine's generated palette header does not parse on the browser's frontend

**Symptom** — a palette shader that declares its sampler with an explicit
binding failed to compile in the browser Studio preview, verbatim:

```
error: Not implemented: variable qualifier
  ┌─ glsl:2:29
  │
2 │ layout(binding = 1) uniform sampler2D palette;
  │                             ^^^^^^^^^
```

Devices and native servers rendered the same project correctly, and nothing was
wrong with the shader — the line is ordinary GLSL.

Scope, stated precisely, because the two shader paths differ:

- **Compute shaders** compose their compiler input as *generated header + user
  source* (`compute_shader_node.rs::compute_glsl_source`), and
  `generate_compute_shader_header` writes exactly this line for every
  `palette`-kind consumed slot. There the failing line is machine-generated: no
  user could avoid it.
- **Visual (pixel) shaders** take their uniforms from the user's own file. The
  form the design doc makes canonical — a bare `uniform sampler2D palette;`,
  no layout qualifier — already worked. Only the explicit-binding spelling
  broke, which is nonetheless what the engine's own palette fixture uses and
  the natural thing to write once the compute header has modelled it.

So the blast radius was "every compute shader with a palette slot, plus any
visual shader that qualifies its sampler", not "all palettes" — but on the one
tier where it bites, it bites at compile time and renders nothing.

## Root cause — a rewrite that declined the only spelling anyone emits

Naga's GLSL-IN has **no combined-sampler type**. `sampler2D` is absent from
`front::glsl::types::parse_type`; it exists only as a Vulkan-style *constructor
builtin*, `sampler2D(tex, samp)`, which associates a separate `texture2D` global
with a separate `sampler` global in a side map (`front/glsl/builtins.rs`,
`ctx.samplers`). An unknown type name lexes as an identifier and dies in the
declaration fallthrough — the error above.

LightPlayer's authored surface is classic combined GLSL, so `lps-frontend`
bridges the gap textually: `parse.rs` rewrites `uniform sampler2D X` into
`uniform texture2D X` plus a synthesized `uniform sampler __lp_samp_X`, and
rewrites `texture(X,` into `texture(sampler2D(X, __lp_samp_X),`.

When the declaration already carries a `layout(…)`, the rewrite re-parses it to
number the companion sampler. That re-parse ended:

```rust
Some((set_v?, bind_v?))     // required BOTH set= and binding=
```

`set` is optional in GLSL — absent means set 0, and Naga itself applies exactly
that default (`ResourceBinding { group: set.unwrap_or(0), … }`). Nothing in the
tree writes a `set` on an authored sampler, and the compute header generator
emits `layout(binding = N)` alone. So the re-parse returned `None`, the whole
line declined the rewrite, passed through verbatim, and hit Naga as an unknown
type.

The masking is the interesting half. Every test of this rewrite used
`layout(set = 0, binding = N)` — a spelling **no producer in the tree emits**.
The only code that writes `set =` for a sampler is the GPU tier's own separate
rewrite (`lp-gfx-wgpu/src/texture_lowering.rs`), which synthesizes it downstream
and never feeds this path. The fixtures were a stand-in for real input and
diverged from it in precisely the dimension that mattered — and the rewrite's
one *unqualified* test (`uniform sampler2D foo;`) exercised the branch that
takes no layout at all, so between them the two shapes of test covered
everything except what a producer actually emits.

Two configurations then hid the result from every suite: the palette contract
was only ever exercised through `LpsGlsl` (devices, native servers — the engine's
own `shader_palette_tests` pin that frontend), while the frontend that could not
parse it, `Naga`, is pinned by exactly one consumer: `fw-browser`'s
`BROWSER_SHADER_FRONTEND`. Palettes existed. Naga existed. Their product shipped
to users and was in no test.

## Fix

`parse_glsl_layout_set_binding` requires `binding` only and defaults `set` to 0,
matching both GLSL and Naga's own default; layout entries that are not
`key = value` are skipped rather than failing the parse. No grammar fork was
needed — the vendored Naga is untouched, and routing palettes to `LpsGlsl` on the
browser (the other candidate fix) is not needed either, so the browser CPU tier
keeps a single frontend.

At fix time the synthesized companion sampler still took `binding + 1` — in a
generated header, the *next slot's* binding number. That overlap was inert (LP
lowering keys globals on `(name, address space)` in declaration order,
`lps-frontend/src/lower.rs::compute_global_layout`, and never reads
`gv.binding`) and was asserted so. A follow-up unification then removed it:
recognition of the declaration now lives in one shared scanner,
`lps_shared::sampler2d_decl::scan_uniform_sampler2d_decls`, consumed by both
the CPU rewrite and the GPU tier's `texture_lowering.rs` — the two textual
rewrites whose divergence this defect is about — and every synthesized binding
numbers past the source's highest explicit `binding = N` (the GPU tier's
scheme), so companions no longer collide with anything.

## Regression coverage

- `lps-frontend/src/parse.rs::uniform_sampler2d_compat_tests::binding_only_layout_is_rewritten`
  — the exact generated spelling, asserting the rewritten text.
- `…::bare_layout_qualifier_does_not_defeat_rewrite` — a layout entry with no
  `=` no longer makes the whole declaration decline.
- `…::generated_palette_header_compiles_tests` — the generated header reaches
  Naga IR, including the case with a palette *between* two other slots (whose
  companion-binding collision the unification later removed outright).
- `lps-frontend/src/lower_texture.rs::generated_palette_header_lowers_to_texture1d_builtin_call`
  — the same header all the way to the height-one texture builtin with a real
  `TextureBindingSpec`.
- `lpc-engine/src/engine/shader_palette_tests.rs::a_palette_uniform_compiles_and_renders_its_baked_strip_through_naga`
  — the browser's whole configuration end to end: the shader node supplies the
  spec, Naga compiles, and the rendered strip matches the `LpsGlsl` one.

That last test needs `lpc-engine`'s non-default `naga` feature, so it rides a
dedicated recipe, `just test-browser-shader-frontend`, folded into `test-rust`.
Left as a bare `#[cfg(feature = "naga")]` it would have compiled to nothing and
passed having run nothing — the failure mode `test-xt-host` already documents.
**CI does not yet run this recipe**; wiring it into the path-gated Validate job
is an open follow-up, and until then the CI-enforced half of the coverage is the
`lps-frontend` tests, which run under default features.

## Lesson

This is `2026-08-01-xtlpn-f32-loses-writes-to-value-parameters` **running in the
opposite direction**, and that makes two on the frontend axis. There, Naga's
habit of copying parameters into fresh locals masked a bug that only `lps-glsl`'s
vreg reuse could expose. Here, `lps-glsl`'s native understanding of `sampler2D`
masked a bug only Naga's textual bridge could expose. Neither frontend is the
reference; each is the other's blind spot, and any contract carried by *both* is
untested until it is run through both. The engine's palette suite is the natural
home for that, which is why the fix parameterizes it by frontend rather than
adding a parallel one.

The narrower lesson is about fixtures. A compatibility shim's tests are only as
good as their fidelity to the real producer, and these were hand-written GLSL
that no generator emits — green forever, about an input that never occurs. When
code exists to consume another component's *output*, at least one test should
take that output from the producer rather than restate it by hand; a restatement
is a stand-in, and it drifts silently because both sides keep passing.
