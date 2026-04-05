# M7: Cleanup

## Goal

Remove all old crates and code that were replaced by LPVM. Ensure everything
builds and passes with no dead code.

## What To Delete

### Crates to remove

| Crate          | Replaced by   | Notes                                                    |
|----------------|---------------|----------------------------------------------------------|
| `lp-glsl-abi`  | `lpvm`        | All types moved to `lpvm`                                |
| `lp-glsl-exec` | `lpvm` traits | `GlslExecutable` replaced by `LpvmModule`/`LpvmInstance` |
| `lps-types`    | `lpir`        | Types absorbed into `lpir` in M1                         |

### Code to remove from other crates

| Location                               | What                            | Replaced by      |
|----------------------------------------|---------------------------------|------------------|
| `lpir-cranelift` (most of it)          | JIT compilation                 | `lpvm-cranelift` |
| `lpir-cranelift` `riscv32-emu` feature | Object compile + link + emulate | `lpvm-rv32`      |
| `lp-glsl-wasm`                         | WASM emission                   | `lpvm-wasm`      |

`lpir-cranelift` may become empty after extraction. If so, delete it.
`lp-glsl-wasm` is fully replaced by `lpvm-wasm`. Delete it.

### Re-export shims

If any shim crates were created during M1 for backward compatibility, remove
them now that all consumers have migrated.

## Workspace Cargo.toml

Remove deleted crates from `[workspace.members]` and `default-members`.

Verify the member list is correct — no references to deleted paths.

## What To Verify

### Full build

```bash
# Default members
cargo check

# Embedded
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Host
cargo check -p lp-server
```

### All tests

```bash
# Filetests (all backends)
cargo test -p lp-glsl-filetests

# Firmware tests
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu

# Server tests
cargo test -p lp-server --no-run

# LPIR tests
cargo test -p lpir
```

### No dead code

```bash
# Check for warnings
cargo check 2>&1 | grep "warning"

# Check for references to deleted crates
# (search Cargo.toml files for old crate names)
```

### No orphaned files

Check that no source files reference deleted crates in imports or comments.

## What NOT To Do

- Do NOT delete anything that still has dependents. Verify all references are
  gone before deleting.
- Do NOT rush this. Each deletion should be verified with a build.
- Do NOT forget to update `web-demo` — it depends on `lp-glsl-wasm` and needs
  to switch to `lpvm-wasm`.

## Done When

- Old crates (`lp-glsl-abi`, `lp-glsl-exec`, `lps-types`) are deleted
- `lpir-cranelift` is deleted or reduced to minimal shim
- `lp-glsl-wasm` is deleted
- No workspace member references deleted paths
- Full build passes (default, embedded, host)
- All tests pass
- No dead code warnings related to the migration
- `web-demo` updated to use `lpvm-wasm`
