## Scope of phase

- **Remove `Backend::Cranelift`** and the **`cranelift.q32`** target from the
  filetest model.
- Add **`Backend::Jit`** and **`Backend::Rv32`** (if not already present from
  earlier work) with static **`Target`** rows per `00-design.md`.
- **`ALL_TARGETS`:** `[wasm.q32, jit.q32, rv32.q32]` (order arbitrary but
  document); **`Target::from_name`** searches **`ALL_TARGETS`**.
- **`DEFAULT_TARGETS`:** **`[jit.q32]` only** — fast local `cargo test` /
  default app runs.
- Update **`Display` for `Backend`**, **`Target::name()`**, and **unit tests**
  (`target/mod.rs`, `target/display.rs`).
- **`parse_annotation::parse_backend`:** support **`jit`**, **`rv32`**,
  **`wasm`**; **remove `cranelift`** (update every annotation test and corpus
  line in phase 05 if not here).
- Fix exhaustive **`match`** on **`Backend`** across the crate.

## Code organization reminders

- Keep **`ALL_TARGETS`** and **`DEFAULT_TARGETS`** adjacent with a short comment
  on **CI vs local** policy.
- **`Backend`** / **`Target`** code should not assume **`lps-cranelift`**;
  execution types come from **`lps-exec`** / **`lpvm`** after phase
  **04** (see **`04-compile-dispatch-and-cargo.md`**).

## Implementation details

- **`lib.rs` / runner:** default invocations use **`DEFAULT_TARGETS`**; CI passes
  expanded list (env or duplicate test target — see phase 05).
- **`lps-filetests-app`:** help text lists **`from_name`** targets =
  **`ALL_TARGETS`**.

## Tests

- `Target::name()` roundtrip for all three; `from_name` errors list all valid.
- `parse_annotation_line` with `backend=jit` / `rv32` / `wasm`; no `cranelift`.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lps && cargo test -p lps-filetests --lib
```

`cargo +nightly fmt`.
