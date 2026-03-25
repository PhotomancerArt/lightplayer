## Scope of phase

Port **`link_and_verify_builtins`** semantics from
`lp-glsl-cranelift/src/backend/codegen/builtins_linker.rs` into
`lpir-cranelift` as **`object_link.rs`** (or similar), behind the same feature
flag.

Inputs: **shader object bytes** + **builtins executable bytes** (embedded at
compile time via `build.rs`, same pattern as old crate’s
`include!(concat!(env!("OUT_DIR"), "/lp_builtins_lib.rs"))`).

Output: **`ElfLoadInfo`** (or equivalent) from `lp_riscv_elf::load_elf` +
`load_object_file`.

## Code organization reminders

- Keep verification logic readable; log lines can follow existing `log::debug!`
  style if `log` is added as optional dep, or omit logging in V1 for simplicity.
- **TODO** only for genuinely temporary hacks (document in cleanup phase).

## Implementation details

- **BuiltinId iteration:** use `lp_glsl_builtin_ids::BuiltinId::all()` and
  `name()` like the old linker — ensures symbol names match declared builtins.
- **Empty builtins blob:** return a clear `CompilerError` / `CompileError` variant
  telling developers to run `scripts/build-builtins.sh` (same message spirit as
  old crate).
- **build.rs:** when feature enabled, resolve path to `lp-glsl-builtins-emu-app`
  artifact; when disabled, `build.rs` should be a no-op or not reference
  missing paths (Cargo always runs `build.rs` — use feature env from
  `CARGO_FEATURE_*` in `build.rs` if needed).

## Tests

- Integration test behind feature + **ignored** or **conditional** if CI lacks
  builtins binary: document in test with `#[ignore = "requires builtins ELF"]`
  or use a tiny pre-checked-in test object only for **link API** smoke (prefer
  matching repo policy — if no binary in tree, ignore is OK).

- Minimum: unit test that **mock** or **skip** if bytes empty; or run full link
  in developer environments only.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo test -p lpir-cranelift --features riscv32-emu
```

If full link tests are ignored, still require `cargo check` with feature on.

`cargo +nightly fmt`.
