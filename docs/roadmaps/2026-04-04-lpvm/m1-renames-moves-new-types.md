# M1: Renames, Moves, and New Types

## Repository state

Renames may already be in progress (RustRover, find/replace). **This document
describes the target layout.** If paths on disk still say `lp-glsl-*`, map them
mentally to the `lps-*` names in [overview.md](overview.md).

## Goal

Establish the **three-layer naming** and crate boundaries:

1. **`lpsc-shared`** — logical shader types (rename/evolution of `lp-glsl-core`).
2. **`lpvm`** — VM/runtime types, traits, values, layout, VMContext.
3. **`lpir`** — unchanged role: **scalarized IR only** (no absorption of
   `Type` / `FunctionSignature` from the shader layer).

No new backends in this milestone. Old crates may remain as shims until later
milestones.

## Context for Agents

Mechanical moves and renames: human often uses RustRover; agent fixes imports,
`Cargo.toml`, and compile errors.

**Do not** delete `lpsc-shared` or old ABI crates in this milestone unless the
human explicitly finishes the shim strategy.

## Layer model (critical)

- **`lpir`** is **scalarized**. It uses `IrType` (F32, I32, Pointer), not logical
  vec3/mat4 as an IR-level type system.
- **`lpsc-shared`** holds **`LpsType`**, **`LpsFunctionSignature`**, **`LpsParameter`**,
  **`LpsParamQualifier`** — what the **frontend** produces and what **callers**
  use for GLSL-style semantics. Same types apply to future WGSL, etc.
- **`lpvm`** holds runtime values (`LpvmValue`), layout, VMContext, and **LPVM
  traits**. It **depends on** `lpsc-shared` for metadata/signatures and on `lpir`
  for `IrModule` / codegen-facing IR.

Wrong: moving `LpsType` into `lpir` “because both are types.” Right: keep logical
types in **`lpsc-shared`**.

## What Moves Where

### `lpsc-shared` (shader layer — rename from `lp-glsl-core`)

| Transitional crate | Target package | Public types (target names)                                                           |
|--------------------|----------------|---------------------------------------------------------------------------------------|
| `lp-glsl-core`     | `lpsc-shared`  | `LpsType`, `LpsStructId`, `LpsFunctionSignature`, `LpsParameter`, `LpsParamQualifier` |

Use a consistent **`Lps*`** prefix for this crate’s public types so they never
collide with `lpir::IrType`, `lpvm::LpvmValue`, or Cranelift’s `Type`.

**Dependents** (update imports in this milestone as renames land): `lps-naga`,
`lps-filetests`, any crate that still depended on `lp-glsl-exec` for signatures
(see below).

### `lpvm` (new crate — absorbs `lpvm` + replaces `lp-glsl-exec` traits)

Create `lpvm/lpvm/` at repo root. `#![no_std]` with `extern crate alloc`.

**Dependencies:**

```toml
[dependencies]
lpir = { path = "...", default-features = false }
lpsc-shared = { path = "...", default-features = false }
```

Paths: use whatever directory layout exists after renames (`lp-glsl/lpir`,
`lps/lpsc-shared`, etc.).

**From `lpvm` (rename map):**

| Old name                       | New name                                        | Notes                                                                                                                                 |
|--------------------------------|-------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------|
| `GlslValue`                    | `LpvmValue`                                     | Runtime value enum                                                                                                                    |
| `GlslType` (metadata / layout) | `LpvmType` or align with `lpsc-shared`          | If duplicate with `LpsType`, **deduplicate or split**: layout-only vs logical type — decide in implementation; document in crate docs |
| `GlslData`                     | `LpvmData`                                      |                                                                                                                                       |
| `GlslModuleMeta`               | `LpvmModuleMeta`                                | Likely references `lpsc-shared` for function signatures                                                                               |
| `GlslFunctionMeta`             | `LpvmFunctionMeta`                              |                                                                                                                                       |
| `GlslParamMeta`                | `LpvmParamMeta`                                 |                                                                                                                                       |
| `GlslParamQualifier` (ABI)     | `LpvmParamQualifier`                            | **Not** the same as `LpsParamQualifier` unless you intentionally unify                                                                |
| `LayoutRules`, `StructMember`  | `LpvmLayoutRules`, `LpvmStructMember`           |                                                                                                                                       |
| `VmContext`                    | `LpvmVmContext`                                 |                                                                                                                                       |
| Paths, layout fns, constants   | `lpvm::path`, `lpvm::layout`, `lpvm::vmcontext` |                                                                                                                                       |

**From `lp-glsl-exec`:**

Replace **`GlslExecutable`** with new traits (design in this milestone;
implementations in M2–M4):

| Trait          | Role                               |
|----------------|------------------------------------|
| `LpvmModule`   | Compiled artifact; can instantiate |
| `LpvmInstance` | Execution + calls + VMContext      |
| `LpvmMemory`   | Linear memory for an instance      |

### Diagnostics / errors

Prefer **`LpvmError`** inside `lpvm` rather than depending on a “glsl”-named
diagnostics crate long-term. During migration, temporary bridges to
`lps-diagnostics` / old `lp-glsl-diagnostics` are OK.

## Crate layout (`lpvm` core)

```
lpvm/
└── lpvm/
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── value.rs
        ├── data.rs
        ├── metadata.rs      # may use lpsc-shared heavily
        ├── layout.rs
        ├── vmcontext.rs
        ├── path.rs
        ├── path_resolve.rs
        ├── value_path.rs
        ├── error.rs
        ├── module.rs        # LpvmModule
        ├── instance.rs      # LpvmInstance
        └── memory.rs        # LpvmMemory
```

## Workspace `Cargo.toml`

Add `lpvm/lpvm` and `lpsc-shared` (or transitional path) to members as they appear.

## What NOT To Do

- Do NOT move **`LpsType` / `LpsFunctionSignature`** into **`lpir`**.
- Do NOT implement LPVM backends in this milestone (only traits + types).
- Do NOT migrate **`lps-filetests`** or **`lp-engine`** to call new backends yet
  (M5–M6).
- Do NOT change **`lpir-cranelift`** / **`lpvm-wasm`** predecessor / **`lp-riscv-emu`**
  for backend behavior yet (M2–M4).

## Dependents to keep in mind

**Will eventually use `lpvm` instead of `lpvm`:**

- `lp-engine`, `lps-builtins`, `lps-naga`, `lpir-cranelift`, `lps-filetests`,
  `lp-glsl-exec` (until removed)

**Use `lpsc-shared` for logical signatures:**

- `lps-naga`, `lps-filetests`, `lp-glsl-exec` / future test harness,
  anything that describes user-visible function types

## Done When

- `lpsc-shared` exists (or transitional name with documented alias) with **`Lps*`**
  logical types; **no** dependency on `lpir` or `lpvm`
- `lpvm` exists, **`no_std` + `alloc`**, depends on **`lpir` + `lpsc-shared`**
- `LpvmModule` / `LpvmInstance` / `LpvmMemory` defined (signatures may still be
  refined in M2)
- `LpvmValue`, `LpvmData`, VMContext, layout helpers present
- Workspace and embedded checks pass (see overview / AGENTS.md for fw-esp32
  command)
