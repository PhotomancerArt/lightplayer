# M7: Cleanup

## Goal

Remove obsolete crates and dead code after LPVM migration. Full build + tests
green.

## Repository state

Package names on disk may still mix `lp-glsl-*` and `lps-*`. Delete only after
**no remaining dependents**. Grep `Cargo.toml` and `use` statements before
removing workspace members.

## Crates to remove (typical)

| Obsolete                                    | Replaced by                   | Notes                        |
|---------------------------------------------|-------------------------------|------------------------------|
| `lpvm`                                      | `lpvm`                        | Runtime ABI + values         |
| `lp-glsl-exec`                              | `lpvm` traits                 | `GlslExecutable` → LPVM      |
| `lpir-cranelift` (bulk)                     | `lpvm-cranelift`, `lpvm-rv32` | Split JIT vs emu object path |
| `lp-glsl-wasm` (or transitional WASM crate) | `lpvm-wasm`                   | Emission + runtime           |

**Keep `lps-shared`** (and the whole **`lps-*` shader layer**). Do **not** delete
the logical-type crate — it is not absorbed into `lpir`.

## Workspace

Remove deleted paths from `[workspace.members]` / `default-members`.

## Verification commands

Adjust **`-p` package names** if crates are not yet renamed on disk.

```bash
cargo check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lp-server
cargo test -p lps-filetests          # or current filetests package name
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
cargo test -p lpir
cargo test -p lp-server --no-run
```

## Web / demo

**`web-demo`** (or `lp-app/web-demo`): switch from old WASM emission crate to
**`lpvm-wasm`** (or `lps-*` wrapper if one exists). Verify wasm32 build if
applicable.

## What NOT To Do

- Do NOT delete **`lps-shared`** or **`lps-naga`** — they are the shader layer,
  not leftovers from ABI/exec migration.
- Do NOT delete a crate that still appears in any `Cargo.toml`.

## Done When

- No references to removed ABI/exec/JIT-monolith crates
- All checks/tests above pass
- `web-demo` (if present) points at new WASM stack
