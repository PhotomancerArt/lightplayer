# Phase 5: Cleanup, validation, plan summary

## Cleanup & validation

- Grep diff for `TODO`, `dbg!`, `println!`, commented-out relational code; remove stray debug.
- `cargo +nightly fmt` on touched crates (`lp-glsl-naga`, filetests if edited).
- `cargo clippy -p lp-glsl-naga -D warnings` (fix or allow with narrow justification only if
  required).

```bash
cd lp2025
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

### Milestone I corpus — all three backends

Run the **Tier A** paths from [`expected-passing-tests.md`](./expected-passing-tests.md) on
**each** target before calling the milestone done:

```bash
TIER_A="builtins/common-isnan.glsl builtins/common-isinf.glsl \
  matrix/mat2/op-equal.glsl matrix/mat2/op-not-equal.glsl \
  matrix/mat3/op-equal.glsl matrix/mat3/op-not-equal.glsl \
  matrix/mat4/op-equal.glsl matrix/mat4/op-not-equal.glsl"
for t in jit.q32 wasm.q32 rv32.q32; do
  echo "=== $t ==="
  ./scripts/glsl-filetests.sh --target "$t" $TIER_A
done
```

Re-run **Tier B** paths (from [`summary.md`](./summary.md)) the same way.

### Full filetest matrix (regression)

```bash
./scripts/glsl-filetests.sh --summary
./scripts/glsl-filetests.sh --target wasm.q32 --summary
./scripts/glsl-filetests.sh --target rv32.q32 --summary
```

Or use `just test-filetests` if your workflow runs the three-target sweep.

Optional narrower regression (still **per target** if you use it):

```bash
for t in jit.q32 wasm.q32 rv32.q32; do
  ./scripts/glsl-filetests.sh --target "$t" "matrix/*op-*equal*" \
    builtins/common-isnan.glsl builtins/common-isinf.glsl
done
```

## Plan cleanup

- Write [`summary.md`](./summary.md): bullets for shipped changes; **Tier B** file list; confirm
  Tier A + Tier B passed **jit / wasm / rv32** (see [`expected-passing-tests.md`](./expected-passing-tests.md));
  known deferrals (Milestone II+).
- When the milestone is fully done and merged, move this directory to `docs/plans-done/` per team
  convention.

## Commit

Use Conventional Commits, e.g.:

```
feat(glsl-naga): relational expr types and Q32 isnan/isinf lowering

- Add expr_type_inner / expr_scalar_kind for Expression::Relational
- Lower isnan/isinf to false lanes per docs/design/q32.md §6
- Fix common-isnan/common-isinf filetests; unmark matrix equality and bvec relational cases
```

Single commit for the milestone is fine unless you prefer per-phase commits (then keep each phase
building green).
