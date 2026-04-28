# Phase 7: Q32 edge tests — `@unsupported`

## Scope of phase

Mark filetests that require **IEEE NaN / Inf** or **domain behavior impossible in Q16.16** with
`@unsupported(float_mode=q32, reason="…")` so they do not count as regressions on the embedded
path. Per [00-notes](./00-notes.md), this is **not** `@broken` — Q32 **cannot** implement that
semantics by design.

## Code organization reminders

- Use a **short, factual** `reason=` string (quoted).
- Prefer **per-`// run:`** annotations only if cases differ; otherwise file-level is fine.

## Implementation details

Target files (from audit):

- `builtins/edge-trig-domain.glsl`
- `builtins/edge-exp-domain.glsl`
- `builtins/edge-nan-inf-propagation.glsl`
- `builtins/edge-precision.glsl` — only if still failing for Q32-vs-float tolerance after other
  work; if fixable in Q32, fix instead of annotating.

Example shape:

```glsl
// @unsupported(float_mode=q32, reason="Q32 has no IEEE NaN or Inf")
```

After edits, confirm the runner reports them as **unsupported**, not **failed**.

## Validate

```bash
./scripts/filetests.sh builtins/edge-trig-domain.glsl builtins/edge-exp-domain.glsl builtins/edge-nan-inf-propagation.glsl
cargo test -p lps-filetests
```

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
