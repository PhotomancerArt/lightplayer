# Phase 1 — LPIR `CompilerConfig`

## Scope of phase

Add **`lpir::compiler_config`**: **`CompilerConfig`**, **`InlineConfig`**, **`InlineMode`**, **`ConfigError`**, and **`CompilerConfig::apply`**, matching the data layout and key set in `docs/roadmaps/2026-04-15-lpir-inliner/m1-optpass-filetests.md`. Implement **`core::str::FromStr`** for **`InlineMode`** (`auto`, `always`, `never` — pick consistent lowercase spelling in **`from_str`** and document it).

Export from **`lib.rs`**. No backend or filetest changes yet.

## Code Organization Reminders

- Prefer one concept per file; **`compiler_config.rs`** holds the whole public surface for this phase.
- Entry points and types first; helper fns at the bottom if any.
- Keep **`#![no_std]`** + **`alloc`** only as needed (e.g. **`String`** in errors — use **`&str`** / static messages if avoiding **`String`**, or align with existing **`lpir`** error patterns).

## Implementation Details

- **`ConfigError`**: support at least **`UnknownKey`**, **`InvalidValue`** (duplicate keys are enforced in the **filetest harness**, not in **`apply`**).
- **`CompilerConfig::default()`** / **`InlineConfig::default()`** per roadmap defaults.
- **`apply(&mut self, key: &str, value: &str)`** — match arms for keys listed in roadmap (`inline.mode`, `inline.small_func_threshold`, `inline.max_growth_budget`, `inline.module_op_budget`). Either add **`inline.always_inline_single_site` → `bool`** or document that it is default-only until a key exists.

### Tests (`lpir` crate)

- **`apply`** success for valid pairs; failure for unknown key and bad parse.
- **`InlineMode`** parsing round-trip.

## Validate

```bash
cargo test -p lpir
```
