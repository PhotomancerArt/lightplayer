## Scope of phase

- **Corpus migration:** replace **`@*(backend=cranelift)`** (and any
  **`parse_backend`** uses of `cranelift`) with **`jit`** or **`rv32`** per test
  intent (host LPIR vs RV32 ISA).
- **Run corpus** on **`jit.q32`** (default), then **`wasm.q32`** and **`rv32.q32`**
  for CI parity.
- **README:** document **`DEFAULT_TARGETS` = `jit.q32` only**; document **CI**
  running **`wasm.q32`** + **`rv32.q32`** (and how to reproduce locally, e.g.
  `lp-glsl-filetests-app test --target wasm.q32` for each).
- **CI config** (workspace root `.github` or equivalent): add or extend job steps
  so the **full three-target** matrix runs on push/PR; keep default Rust test
  binary fast if it only sees **`DEFAULT_TARGETS`** — may need **explicit second
  command** for extra targets.

## Code organization reminders

- Prefer scripted **`rg`** passes for annotation migration; review diffs for
  tests that truly needed emulator-only behavior (**`rv32`**) vs generic
  compiler behavior (**`jit`**).

## Implementation details

- If **`rv32`** is not green yet, CI can temporarily **`continue-on-error`** or
  run **`jit`+`wasm`** only until V1 lands — document; remove waiver when green.

## Tests

- Full **`cargo test -p lp-glsl-filetests --test filetests`** locally with
  **`--target`** or env to sweep all targets when ready.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo test -p lp-glsl-filetests --test filetests
# Plus CI-equivalent multi-target invocation once wired
```

`cargo +nightly fmt`.
