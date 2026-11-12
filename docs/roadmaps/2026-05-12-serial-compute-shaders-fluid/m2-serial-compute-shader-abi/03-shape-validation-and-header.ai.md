# Phase 3: Shape Validation And Header Generation

## Scope Of Phase

Connect the M1 shader slot shape model to the M2 compute shader ABI.

In scope:

- Update generated compute headers so produced slots are private globals, not
  GLSL `out` variables.
- Add validation between `ComputeShaderDef`/`ShaderSlotDef` and lowered shader
  metadata.
- Support the first sentinel-map shape validation.
- Add tests for fluid emitter fixed-array output.

Out of scope:

- Materializing sentinel arrays into `SlotMap`.
- Engine/node integration.
- UI header regeneration.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Tests go at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/shader/shader_header_gen.rs`
- `lp-core/lpc-model/src/nodes/shader/shader_slot_def.rs`
- `lp-core/lpc-model/src/nodes/shader/compute_shader_def.rs`
- `lp-core/lpc-model/src/nodes/fluid/fluid_emitter.rs`
- `lp-shader/lp-shader/src/compile_compute_desc.rs`
- `lp-shader/lp-shader/src/compute_abi.rs`
- `lp-shader/lp-shader/src/compute_shader.rs`

Expected changes:

1. Change generated produced slots from:

   ```glsl
   out FluidEmitter emitters[4];
   ```

   to:

   ```glsl
   // produced: emitters
   FluidEmitter emitters[4];
   ```

2. Keep consumed slots as:

   ```glsl
   // consumed: time
   layout(binding = 0) uniform float time;
   ```

3. Add shader-local ABI validation helpers that can compare:

   - model slot value refs to `lps_shared::LpsType`;
   - consumed value slots to `meta.uniforms_type`;
   - produced value slots to `meta.globals_type`;
   - sentinel maps to fixed `LpsType::Array`.

4. Native shape conversion should use the `SlotShapeRegistry` to resolve
   `lp::fluid::Emitter`, then translate its `LpType` to equivalent `LpsType`.
   Keep this conversion small and explicit for M2.

5. For `mapping = { kind = "sentinel", len, key, empty_key }`, validate:

   - produced global is a fixed array of the expected length;
   - array element type equals the value shape;
   - key field exists on the element struct;
   - key type is `u32` for now.

6. Add tests that compile a generated-header-style compute shader producing:

   ```glsl
   FluidEmitter emitters[4];
   void tick() {
     emitters[0].id = 1u;
     emitters[0].pos = vec2(0.25, 0.75);
   }
   ```

   Then read `emitters` as `LpsValueF32::Array`.

Diagnostics:

- Missing uniform/global should name the slot.
- Type mismatch should show expected and actual shape.
- Unsupported mapping should name the mapping kind.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lp-shader
cargo check -p lpc-engine
```
