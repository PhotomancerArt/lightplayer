# Scope of Phase

Add the builtin ABI foundation needed for texture sampler builtins.

This phase should make the generated builtin machinery able to see and link
texture builtins that:

- live in a texture builtin namespace/module;
- return `vec4`-shaped Q32 data through a result pointer;
- may need descriptor/texture pointer words as arguments;
- work across native, Cranelift, WASM, and emulator builtin resolution paths.

Out of scope:

- Implementing full sampler math for all formats.
- Frontend `texture()` lowering.
- Adding the final sampling filetests.
- Refactoring unrelated LPFN builtin behavior.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this phase.

# Implementation Details

Relevant files and systems:

- `lp-shader/lps-builtins/src/builtins/texture/`
- `lp-shader/lps-builtins-gen-app/src/main.rs`
- `lp-shader/lps-builtins-gen-app/src/native_dispatch_codegen.rs`
- `lp-shader/lps-builtin-ids/src/lib.rs` (generated)
- `lp-shader/lps-builtin-ids/src/glsl_builtin_mapping.rs` (generated if needed)
- `lp-shader/lps-builtins/src/jit_builtin_ptr.rs` (generated)
- `lp-shader/lpvm-cranelift/src/builtins.rs`
- `lp-shader/lpvm-cranelift/src/generated_builtin_abi.rs` (generated)
- `lp-shader/lpvm-cranelift/src/emit/call.rs`
- `lp-shader/lpvm-wasm/src/emit/imports.rs`
- `lp-shader/lpvm-wasm/src/emit/ops.rs`
- `lp-shader/lpvm-wasm/src/rt_wasmtime/native_builtin_dispatch.rs` (generated)
- `lp-shader/lps-builtins-emu-app/src/builtin_refs.rs` (generated)
- `lp-shader/lps-builtins-wasm/src/builtin_refs.rs` (generated if present)

The existing builtin system discovers `#[unsafe(no_mangle)] pub extern "C"`
functions in `lps-builtins/src/builtins/` and generates builtin IDs, ABI tables,
module lists, and dispatch code.

Texture builtins should be discovered under a new builtin module namespace, for
example by exported symbol prefixes like:

```text
__lp_texture2d_rgba16_unorm_q32
__lp_texture1d_rgba16_unorm_q32
```

If the generator currently only recognizes `__lps_`, `__lp_lpir_`,
`__lp_glsl_`, `__lp_vm_`, and `__lp_lpfn_`, extend it to recognize texture
builtins. Prefer a clear builtin module name such as `"texture"`.

Add minimal placeholder externs if needed to prove generation and linkage. They
must be real implementations, not panic/stub behavior in production paths. For
this ABI phase, a placeholder can write a deterministic vec4 such as
`(0, 0, 0, Q32_ONE)` if and only if no frontend path can call it yet. Remove or
replace that placeholder in phase 3.

Result-pointer handling:

- Existing LPFN vector builtins use a result-pointer ABI: the extern function
  takes `*mut i32` as the first parameter and writes result lanes.
- `lpvm-wasm` detects result-pointer builtins by comparing logical LPIR
  `return_types` against the generated WASM import signature.
- `lpvm-cranelift` has LPFN-specific result-pointer handling in
  `is_import_result_ptr_builtin`.

Generalize result-pointer handling so it is not hardwired only to `lpfn` if
texture imports need the same pattern. Keep LPFN behavior unchanged.

Import resolution:

- Add texture builtin lookup support for native and Cranelift import resolution.
- Add texture builtin lookup support for WASM import pruning/name resolution.
- Do not make texture builtins masquerade as GLSL math builtins. Keep the
  namespace explicit so diagnostics remain readable.

Frontend import declarations are implemented in a later phase, but this phase
should document and test the expected import shape. A texture import should be
able to declare logical return types `[F32, F32, F32, F32]` while the actual
extern returns through an out pointer.

Regenerate builtin files using the repository's existing command:

```bash
cargo run -p lps-builtins-gen-app
```

or the existing script if that is the current local pattern:

```bash
scripts/build-builtins.sh
```

Do not hand-edit generated files except to inspect them.

Tests to add or update:

- A focused generator test or compile test proving texture builtin IDs are
  generated.
- A focused import-resolution test, if the local crates already have a good
  place for one.
- Existing LPFN vector result-pointer tests must continue to pass.

# Validate

Run:

```bash
cargo run -p lps-builtins-gen-app
cargo check -p lps-builtins
cargo check -p lps-builtin-ids
cargo check -p lpvm-cranelift
cargo check -p lpvm-wasm
```

If generated RV32 builtin artifacts are required by the repo after regeneration,
also run the existing builtins build script and report the exact command used.
