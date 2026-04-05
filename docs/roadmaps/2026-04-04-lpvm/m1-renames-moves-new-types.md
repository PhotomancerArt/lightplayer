# M1: Renames, Moves, and New Types

## Goal

Create the `lpvm` core crate and reorganize existing types into their correct
homes. This is mostly mechanical refactoring — no new functionality, no new
backends. The old and new crates coexist at the end of this milestone.

## Context for Agents

This milestone involves moving types between crates. The human will handle bulk
renames and moves using RustRover's refactoring tools. Agent work is cleanup:
fixing imports, updating Cargo.toml dependencies, ensuring everything compiles.

**Do not** delete old crates in this milestone. They may temporarily exist as
thin re-export shims until consumers are migrated in later milestones.

## What Moves Where

### Into `lpir` (absorbing `lps-types`)

`lps-types` is eliminated. Its types move into `lpir`:

| Old location                   | Old name            | New location                     | New name                     |
|--------------------------------|---------------------|----------------------------------|------------------------------|
| `lps-types::Type`              | `Type`              | `lpir::types::Type`              | Keep `Type` (internal to IR) |
| `lps-types::StructId`          | `StructId`          | `lpir::types::StructId`          | Keep `StructId`              |
| `lps-types::FunctionSignature` | `FunctionSignature` | `lpir::types::FunctionSignature` | Keep                         |
| `lps-types::Parameter`         | `Parameter`         | `lpir::types::Parameter`         | Keep                         |
| `lps-types::ParamQualifier`    | `ParamQualifier`    | `lpir::types::ParamQualifier`    | Keep                         |

Note: `lpir` already has `lpir::types` with `IrType`, `VReg`, `SlotId`, etc.
The `lps-types` types are higher-level (GLSL/shader types like vec3, mat4)
vs `IrType` which is low-level (i32, f32, pointer). Both belong in `lpir` but
at different abstraction levels. Consider a submodule like `lpir::shader_types`
or similar if `lpir::types` gets crowded.

**Dependents to update**: `lp-glsl-exec`, `lp-glsl-filetests` — both depend on
`lps-types` directly and need their imports changed to `lpir`.

### Into `lpvm` (new crate, absorbing `lp-glsl-abi` and `lp-glsl-exec`)

Create `lpvm/lpvm/` at the repo root. `#![no_std]` with `extern crate alloc`.

**From `lp-glsl-abi`:**

| Old name                                                                         | New name                             | Notes                                                                                                                              |
|----------------------------------------------------------------------------------|--------------------------------------|------------------------------------------------------------------------------------------------------------------------------------|
| `GlslValue`                                                                      | `LpvmValue`                          | Enum with scalar/vector/matrix/array/struct variants                                                                               |
| `GlslType` (metadata)                                                            | `LpvmType`                           | Runtime type metadata enum (parallels `lpir::Type` but with layout info like `StructMember`)                                       |
| `GlslData`                                                                       | `LpvmData`                           | Binary data buffer with typed access                                                                                               |
| `GlslDataError`                                                                  | `LpvmDataError`                      |                                                                                                                                    |
| `GlslModuleMeta`                                                                 | `LpvmModuleMeta`                     | Module-level metadata (function list, types)                                                                                       |
| `GlslFunctionMeta`                                                               | `LpvmFunctionMeta`                   | Per-function metadata                                                                                                              |
| `GlslParamMeta`                                                                  | `LpvmParamMeta`                      | Parameter metadata                                                                                                                 |
| `GlslParamQualifier`                                                             | `LpvmParamQualifier`                 | Note: `lps-types` also has `ParamQualifier` — these are different types at different levels. This one is the ABI/metadata version. |
| `LayoutRules`                                                                    | `LpvmLayoutRules`                    | Layout strategy enum                                                                                                               |
| `StructMember`                                                                   | `LpvmStructMember`                   | Struct field descriptor                                                                                                            |
| `VmContext` / `VmContextHeader`                                                  | `LpvmVmContext`                      | repr(C) VMContext header                                                                                                           |
| `PathSegment`, `PathParseError`, `PathError`, `GlslValuePathError`               | `LpvmPathSegment`, etc.              | Path accessors for navigating values                                                                                               |
| Layout functions: `type_size`, `type_alignment`, `array_stride`, `round_up`      | Same names, in `lpvm::layout` module |                                                                                                                                    |
| `minimal_vmcontext`                                                              | Keep name, in `lpvm::vmcontext`      |                                                                                                                                    |
| `parse_path`                                                                     | Keep name, in `lpvm::path`           |                                                                                                                                    |
| VMContext constants: `DEFAULT_VMCTX_FUEL`, `VMCTX_OFFSET_*`, `VMCTX_HEADER_SIZE` | Keep names, in `lpvm::vmcontext`     |                                                                                                                                    |

**From `lp-glsl-exec`:**

The `GlslExecutable` trait is replaced by the new LPVM traits. These are
**new designs**, not direct renames:

| New trait      | Role                                                                                       |
|----------------|--------------------------------------------------------------------------------------------|
| `LpvmModule`   | Compiled code. Immutable after compilation. Can create instances.                          |
| `LpvmInstance` | Running execution context. Owns or references memory + VMContext. Supports function calls. |
| `LpvmMemory`   | Linear memory backing an instance.                                                         |

The trait signatures will be designed during this milestone but implementations
come in M2-M4 (backends).

### `lpvm` dependency on `lpir`

`lpvm` depends on `lpir` for `Type` and `FunctionSignature` (which moved into
`lpir` above). This is the right direction: the runtime knows about IR types,
but the IR doesn't know about the runtime.

### `lpvm` dependency on `lp-glsl-diagnostics`

`lp-glsl-abi` currently depends on `lp-glsl-diagnostics` for `GlslError`. The
`lpvm` crate needs an error type. Options:

1. Depend on `lp-glsl-diagnostics` — keeps the dependency but "glsl" in a
   runtime crate is ugly.
2. Define `LpvmError` in `lpvm` — cleaner, but may need conversion from/to
   `GlslError` during migration.

Prefer option 2. `lpvm` should have its own error type. Conversions can bridge
during the migration period.

## Crate Setup Details

### `lpvm/lpvm/Cargo.toml`

```toml
[package]
name = "lpvm"
version = "0.1.0"
edition = "2024"

[dependencies]
lpir = { path = "../../lp-glsl/lpir", default-features = false }

[features]
default = ["std"]
std = []
parse = []  # GlslValue::parse equivalent, if needed
```

Note: `lpir` is still under `lp-glsl/` for now. It may move to top-level later.
Use a relative path that works with the current directory structure.

### Directory layout

```
lpvm/
└── lpvm/
    ├── Cargo.toml
    └── src/
        ├── lib.rs          # #![no_std], re-exports
        ├── value.rs         # LpvmValue
        ├── data.rs          # LpvmData, LpvmDataError
        ├── metadata.rs      # LpvmType, LpvmModuleMeta, LpvmFunctionMeta, etc.
        ├── layout.rs        # type_size, type_alignment, array_stride
        ├── vmcontext.rs     # LpvmVmContext, constants
        ├── path.rs          # LpvmPathSegment, parse_path
        ├── path_resolve.rs  # PathError
        ├── value_path.rs    # LpvmValuePathError, value path navigation
        ├── error.rs         # LpvmError
        ├── module.rs        # LpvmModule trait
        ├── instance.rs      # LpvmInstance trait
        └── memory.rs        # LpvmMemory trait
```

## Workspace Cargo.toml

Add `"lpvm/lpvm"` to `[workspace.members]` and `default-members`.

## What NOT To Do

- Do NOT implement the traits in this milestone. Trait definitions only.
- Do NOT delete `lp-glsl-abi`, `lp-glsl-exec`, or `lps-types` yet. They
  may exist as re-export shims or be referenced by code not yet migrated.
- Do NOT update `lp-engine` or `lp-glsl-filetests` to use `lpvm` yet. That's
  M5-M6.
- Do NOT change `lpir-cranelift`, `lp-glsl-wasm`, or `lp-riscv-emu` yet.
  That's M2-M4.

## Existing Dependents of Affected Crates

### `lp-glsl-abi` dependents (will need updating in later milestones)

- `lp-engine`
- `lp-glsl-builtins`
- `lp-glsl-naga`
- `lpir-cranelift`
- `lp-glsl-filetests`
- `lp-glsl-exec`

### `lps-types` dependents (update in this milestone)

- `lp-glsl-exec`
- `lp-glsl-filetests`

### `lp-glsl-exec` dependents (will need updating in M5)

- `lp-glsl-filetests`

## Done When

- `lpvm` crate exists at `lpvm/lpvm/` and compiles (`no_std` + `alloc`)
- `LpvmModule`, `LpvmInstance`, `LpvmMemory` traits are defined
- `LpvmValue`, `LpvmData`, `LpvmType`, `LpvmVmContext`, layout functions exist
- `lps-types` types are in `lpir`
- Old crates still exist (possibly as shims)
- Full workspace builds: `cargo check` passes for default members
- Embedded check passes:
  `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`
