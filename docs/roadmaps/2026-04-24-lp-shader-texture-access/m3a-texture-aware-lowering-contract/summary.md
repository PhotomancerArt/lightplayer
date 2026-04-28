### What was built

- `LowerOptions` with owned `texture_specs` map, `lower_with_options`, and `LpsModuleSig::texture_specs` populated after optional M1/M2-style binding validation when the options map is non-empty.
- `lower_texture`: Naga `ImageLoad` path for GLSL `texelFetch`—direct `Texture2D` uniform resolution, spec presence, LOD literal-0-only, multisample/layered rejection, and intentional M3b placeholder diagnostic for otherwise valid fetches.
- Callers (`lp-shader` compile path, `lps-filetests` error and run paths) wired through `lower_with_options`; unit tests, sampler2d metadata tests, Naga-shape test in `lower_texture`, and texture diagnostic filetests cover the contract.

### Decisions for future reference

#### Canonical lowering API for textures

- **Decision:** Use `lower_with_options` and `LowerOptions` (not a dedicated `lower_with_texture_specs`) so future lowering options have a single extension point; keep `lower(&NagaModule)` as the zero-spec convenience wrapper.
- **Why:** Avoids API churn when more compile-time options are added; most product callers already go through `compile_px_desc` where textures are part of the descriptor.
- **Rejected alternatives:** A narrowly named `lower_with_texture_specs` only; making `lower` require specs (breaks texture-free call sites).
- **Revisit when:** Adding the next lowering-time option (e.g. profile or feature flags)—evaluate whether options should hold references instead of owned maps for hot paths.

#### M3a vs M3b boundary

- **Decision:** After validating operand, spec, and LOD, M3a fails with a clear message that the `texelFetch` data path is implemented in M3b (no descriptor loads, address math, or channel conversion).
- **Why:** Keeps M3a strictly contract/diagnostic/metadata; all real load codegen stays in one milestone.
- **Rejected alternatives:** Emitting partial IR or builtins that would duplicate or conflict with M3b’s eventual ABI.
- **Revisit when:** M3b lands—replace placeholder with lowering to the agreed builtin/IL pattern and delete or repurpose placeholder assertions in tests.

#### Operand resolution for v0

- **Decision:** Only direct uniform `Texture2D` globals (after peeling `Load`); no locals, parameters, or aliases.
- **Why:** Matches M1/M2 texture uniform model and avoids speculative dataflow in the frontend.
- **Rejected alternatives:** Tracking sampler provenance through assignment chains.
- **Revisit when:** If GLSL patterns need sampled textures passed through helpers—likely requires explicit ABI rules first.
