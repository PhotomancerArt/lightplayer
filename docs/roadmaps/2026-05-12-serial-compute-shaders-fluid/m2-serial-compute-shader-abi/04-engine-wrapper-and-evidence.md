# Phase 4: Engine Wrapper And Evidence

## Scope Of Phase

Add only the engine-facing wrapper needed to make M3 straightforward, and add
evidence tests/examples that demonstrate the ABI shape clearly.

In scope:

- Add a thin compute compile wrapper in `lpc-engine::gfx` if the Phase 2/3 API
  does not already make M3 obvious.
- Add evidence tests or fixtures using `ComputeShaderDef` and generated header.
- Document the M2 compute ABI in roadmap summary notes.

Out of scope:

- `ComputeShaderNode`.
- Fluid node integration.
- Debug UI or wire protocol changes.

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

- `lp-core/lpc-engine/src/gfx/lp_gfx.rs`
- `lp-core/lpc-engine/src/gfx/lp_shader.rs`
- `lp-core/lpc-engine/src/gfx/host.rs`
- `lp-core/lpc-engine/src/gfx/native_jit.rs`
- `lp-core/lpc-engine/src/gfx/wasm_guest.rs`
- `lp-shader/lp-shader/src/compute_shader.rs`
- `docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m2-serial-compute-shader-abi/summary.md`

Expected changes:

1. If useful, introduce a minimal engine-facing trait/type parallel to the
   visual shader wrapper. Keep it thin:

   - compile compute shader;
   - tick with inputs;
   - read output by name.

2. Do not introduce `ComputeProduct` or a node runtime in this phase.

3. Add an evidence test or fixture that:

   - builds a `ComputeShaderDef`;
   - generates the header;
   - compiles shader source using that header;
   - ticks the shader;
   - reads a produced value.

4. Write `summary.md` with:

   - the final ABI;
   - lifecycle decision: no global reset per tick;
   - entry point decision: `tick()`;
   - known next steps for M3.

## Validate

```bash
cargo fmt --check
cargo test -p lp-shader
cargo test -p lpc-model
cargo check -p lpc-engine
```

