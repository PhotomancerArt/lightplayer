# Phase 5 — Runtime Fixture Allocation And Binding

## Scope of Phase

Allocate texture fixture data in backend shared memory and bind typed
`LpsTexture2DDescriptor` uniform values before each `// run:` executes.

This phase should make texture fixtures real runtime resources for all existing
LPVM filetest backends.

Out of scope:

- Do not implement `texelFetch` or `texture` lowering/execution.
- Do not add wgpu runner support.
- Do not add sidecar image fixtures.
- Do not relax `set_uniform` beyond the typed `Texture2D` behavior from Phase 1.

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
- `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`
- `lp-shader/lps-filetests/src/test_run/run_detail.rs`
- `lp-shader/lps-filetests/src/test_run/set_uniform.rs`
- `lp-shader/lps-filetests/src/test_run/texture_fixture.rs`
- `lp-shader/lpvm/src/engine.rs`
- `lp-shader/lpvm/src/buffer.rs`
- `lp-shader/lp-shader/src/texture_buf.rs`

Current issue:

- `CompiledShader::compile_glsl` creates backend engines locally and returns
  only compiled modules.
- Shared fixture memory is allocated through `LpvmEngine::memory()`, so the
  filetest compiled artifact needs access to the engine memory after compile.

Required behavior:

- For each selected `// run:`, before normal `// set_uniform:` directives and
  before execution:
  - Validate every parsed `texture-spec` has a matching runtime fixture.
  - Validate every fixture name is declared by a spec.
  - Validate fixture format matches the compile-time spec format.
  - Validate `TextureShapeHint::HeightOne` fixtures have `height == 1`.
  - Encode fixture bytes using `texture_fixture.rs`.
  - Allocate backend shared memory using the backend engine memory.
  - Write encoded bytes into the allocation.
  - Build `LpsTexture2DDescriptor { ptr, width, height, row_stride }`.
  - Call `FiletestInstance::set_uniform(name, &LpsValueF32::Texture2D(desc))`.

Design guidance:

- Fixture allocations may be per-run. This is simple and keeps runs isolated.
- Keep allocation handles alive at least until the run completes. A small
  per-run `Vec<LpvmBuffer>` held in the binding helper is enough if the
  allocator requires handles for safety or future cleanup.
- Use `LpvmBuffer::write` to fill memory, respecting its safety contract.
- Use `format.bytes_per_pixel()` and the encoded fixture row stride.
- Keep the descriptor role-neutral; do not call it a uniform-only type in new
  code.

Possible shape in `filetest_lpvm.rs`:

```rust
impl CompiledShader {
    pub(crate) fn alloc_shared(&self, size: usize, align: usize) -> anyhow::Result<LpvmBuffer> {
        // match backend variant and call retained engine.memory().alloc(...)
    }
}
```

If retaining engines inside `CompiledShader` is the cleanest path, do that
rather than adding ad hoc allocation APIs to instances.

Be careful with backend ownership:

- Cranelift JIT, RV32 emu, native RV32 emu, and Wasm currently have different
  engine/module types.
- Preserve existing module lifetime behavior. Do not drop a module earlier than
  today.
- If Wasmtime shared memory has special allocation behavior, follow the existing
  `LpvmEngine::memory()` implementation rather than writing directly into wasm
  memory from scratch.

Tests:

- Add unit tests for runtime fixture validation where possible:
  - missing runtime fixture.
  - extra fixture without spec.
  - fixture format mismatch.
  - height-one mismatch.
- Add a minimal run filetest that binds a fixture and is marked
  `@unimplemented` for sampling if needed. It should still prove compile and
  binding setup runs before the expected unimplemented execution path.

## Validate

Run from repo root:

```bash
cargo test -p lps-filetests texture_fixture
cargo test -p lps-filetests --test filetests -- --ignored --nocapture
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If the full ignored filetest run is too slow, report that and run targeted
texture filetests with the existing `TEST_FILE=...` mechanism.

