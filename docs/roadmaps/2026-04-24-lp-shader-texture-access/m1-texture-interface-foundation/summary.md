# Milestone 1 — Texture interface foundation (summary)

## What was built

- **Shared vocabulary** (`lps-shared`): `TextureBindingSpec`, `TextureFilter`, `TextureWrap`, and `TextureShapeHint` alongside `TextureStorageFormat`; logical `LpsType::Texture2D` with fixed std430 layout (16 bytes / 4-byte align) for the guest descriptor ABI.
- **Compile descriptor** (`lp-shader`): `CompilePxDesc` carries GLSL source, output format, `CompilerConfig`, and a deterministic `BTreeMap` of texture binding specs by sampler uniform name.
- **Compatibility**: `LpsEngine::compile_px` remains a thin wrapper that builds a descriptor with an empty texture map and calls `compile_px_desc`.
- **Frontend**: GLSL `sampler2D` uniforms lower to `Texture2D` in module metadata; validation enforces a strict match between declared samplers and compile-time specs (no missing or extra bindings; shape/policy rules per milestone).
- **Runtime**: `Texture2DUniform` is the `repr(C)` guest descriptor `{ ptr, width, height, row_stride }` built from `LpsTextureBuf`.
- **Uniform writes**: Scalar/aggregate `set_uniform` paths reject `Texture2D` leaf paths with a dedicated error so callers do not treat texture slots like float uniforms.

## Decisions for future reference

- **`Texture2D` vs descriptor**: `Texture2D` is the logical shader/metadata type. The uniform’s ABI is the four-`u32` descriptor; metadata and diagnostics stay logical (no public “fake struct” of `ptr`/`width` fields in the uniform *type* surface the way a user-defined struct would).
- **Native JIT ABI**: `lpvm-native` scalar word counting treats `LpsType::Texture2D` as **4** words (same footprint as `uvec4` / 16 bytes std430), consistent with the guest descriptor layout.
- **`compile_px`**: Kept as a compatibility wrapper around the descriptor API so existing positional call sites stay stable while new code passes explicit texture specs.
- **Normal scalar uniform writes**: They do not encode or expose texture descriptor fields; texture binding uses the typed helper / dedicated path, not `set_uniform` on a texture leaf.
