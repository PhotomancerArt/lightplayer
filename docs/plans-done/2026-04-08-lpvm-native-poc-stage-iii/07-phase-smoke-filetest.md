## Phase 7: Smoke filetest execution

### Scope

Run a minimal GLSL test through the `rv32lp.q32` backend.

**Test selection:**
- `filetests/scalar/float/op-add.glsl` (first runnable function)
- OR create `filetests/native/smoke-add.glsl` if existing file uses unsupported ops

**Procedure:**
1. Run `./scripts/glsl-filetests.sh <file> rv32lp.q32`
2. If failures: add `// @unimplemented(rv32lp.q32)` to failing lines
3. Goal: at least one `// run:` line reports `PASS`

### Implementation details

```bash
# Build builtins first (required for link)
./scripts/build-builtins.sh

# Run smoke test
./scripts/glsl-filetests.sh filetests/scalar/float/op-add.glsl rv32lp.q32
```

Expected output: `PASS` with numeric result matching expected (e.g., `add(1.0, 2.0)` → `3.0` in Q32).

### Tests

```bash
./scripts/glsl-filetests.sh filetests/scalar/float/op-add.glsl rv32lp.q32
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
