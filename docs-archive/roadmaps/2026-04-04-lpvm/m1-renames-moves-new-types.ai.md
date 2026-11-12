# M1: Renames, Moves, and New Types

## Repository state

**COMPLETED.** The renames are done. Key differences from the original plan:

- `lpvm` crate stayed in `lp-shader/lpvm/` instead of moving to repo root
- `GlslValue`/`GlslType` moved to `lps-shared` as `LpsValue`/`LpsType` (not `LpvmValue`/`LpvmType`)
- `lpvm` crate contains `LpvmData`, `VmContext`, and re-exports from `lps-shared`

## Goal

Establish the **three-layer naming** and crate boundaries:

1. **`lps-shared`** — logical shader types (`LpsType`, `LpsValue`, `LpsFnSig`).
2. **`lpvm`** — VM/runtime data buffers and VMContext.
3. **`lpir`** — unchanged role: **scalarized IR only**.

No new backends in this milestone. Old crates remain as shims until later milestones.

## Layer model (critical)

- **`lpir`** is **scalarized**. It uses `IrType` (F32, I32, Pointer), not logical
  vec3/mat4 as an IR-level type system.
- **`lps-shared`** holds **`LpsType`**, **`LpsFnSig`**, **`FnParam`**,
  **`ParamQualifier`** — what the **frontend** produces and what **callers**
  use for GLSL-style semantics.
- **`lpvm`** holds runtime data buffers (`LpvmData`), VMContext, and re-exports
  `LpsValue`/`LpsType` from `lps-shared` for convenience.

## What Moves Where

### `lps-shared` (shader layer — new crate)

| Source            | Target package | Public types (actual names)                             |
|-------------------|----------------|---------------------------------------------------------|
| `lpvm` types      | `lps-shared`   | `LpsType`, `LpsValue`, `StructMember`, `LayoutRules`    |
| `lpvm` signatures | `lps-shared`   | `LpsFnSig`, `FnParam`, `ParamQualifier`, `LpsModuleSig` |
| `lpvm` path utils | `lps-shared`   | `path`, `path_resolve`, `value_path` modules            |

**Note:** Original plan suggested `GlslValue` → `LpvmValue`, but we moved these
logical types to `lps-shared` with `Lps*` prefix instead. `LpsValue` is a logical
shader value (vec3, mat4), not a VM runtime concept.

**Dependents:** `lps-frontend`, `lps-filetests`, `lpvm`, `lps-exec`

### `lpvm` (refactored — stayed in `lp-shader/`)

**Actual location:** `lp-shader/lpvm/` (not at repo root as originally planned).

**Dependencies:**

```toml
[dependencies]
lps-shared = { path = "../lps-shared" }
```

**Types in `lpvm`:**

| Name              | Role                                                      |
|-------------------|-----------------------------------------------------------|
| `LpvmData`        | Byte buffer with layout/path access (runtime shader data) |
| `DataError`       | Error type for data operations                            |
| `VmContext`       | Fixed-layout header for VM contexts                       |
| `VmContextHeader` | Type alias for `VmContext`                                |

**Re-exports from `lps-shared`:**

- `LpsValue`, `LpsType`, `StructMember`, `LayoutRules`
- `LpsFnSig`, `FnParam`, `ParamQualifier`
- Path modules: `parse_path`, `LpsPathSeg`, `PathParseError`
- Layout functions: `type_size`, `type_alignment`, `array_stride`, `round_up`

### Legacy crates (in `lp-shader/legacy/`)

| Crate            | Status                                               |
|------------------|------------------------------------------------------|
| `lps-exec`       | Still has `GlslExecutable` trait (used by filetests) |
| `lpvm-cranelift` | Still contains JIT compiler                          |
| `lps-wasm`       | Still contains WASM backend                          |

## Crate layout (actual)

```
lp-shader/
├── lps-shared/
│   └── src/
│       ├── lib.rs          # LpsType, LpsValue, re-exports
│       ├── types.rs          # LpsType, StructMember, LayoutRules
│       ├── lps_value.rs     # LpsValue enum
│       ├── sig.rs            # LpsFnSig, FnParam, ParamQualifier, LpsModuleSig
│       ├── layout.rs         # std430 layout functions
│       ├── path.rs           # Path parsing (LpsPathSeg)
│       ├── path_resolve.rs   # Type path resolution
│       └── value_path.rs     # Value path projection
├── lpvm/
│   └── src/
│       ├── lib.rs            # Re-exports + LpvmData, VmContext
│       ├── data.rs           # LpvmData implementation
│       ├── data_error.rs     # DataError
│       └── vmcontext.rs      # VmContext, VmContextHeader, constants
├── lpir/
│   └── src/                  # Unchanged (IrType, IrModule, etc.)
└── legacy/
    ├── lps-exec/             # GlslExecutable trait
    ├── lpvm-cranelift/       # JIT compiler
    └── lps-wasm/             # WASM backend
```

## Workspace `Cargo.toml`

Already updated:

- `lp-shader/lps-shared` in members
- `lp-shader/lpvm` in members
- All `lps-*` crates using new naming

## What Was NOT Done (Deferred to Later Milestones)

- **No** `LpvmModule` / `LpvmInstance` / `LpvmMemory` traits yet — these come in M2-M4
- **No** `LpvmModuleMeta` / `LpvmFunctionMeta` / `LpvmParamMeta` — metadata design deferred
- **No** move of `lpvm` to repo root — stayed in `lp-shader/lpvm/`
- **No** `LpvmError` type — still using `DataError` and `lps-diagnostics`
- `lps-exec` still exists with `GlslExecutable` (M5/M6 migration)

## Types Summary

| Old Name                | Location | New Name       | Notes                 |
|-------------------------|----------|----------------|-----------------------|
| `GlslValue`             | `lpvm`   | `LpsValue`     | Moved to `lps-shared` |
| `GlslType`              | `lpvm`   | `LpsType`      | Moved to `lps-shared` |
| `GlslData`              | `lpvm`   | `LpvmData`     | Stayed in `lpvm`      |
| `StructMember`          | `lpvm`   | `StructMember` | Moved to `lps-shared` |
| `LayoutRules`           | `lpvm`   | `LayoutRules`  | Moved to `lps-shared` |
| `VmContext`             | `lpvm`   | `VmContext`    | Kept name, in `lpvm`  |
| `GlslFunctionSignature` | `lpvm`   | `LpsFnSig`     | Moved to `lps-shared` |
| `GlslParameter`         | `lpvm`   | `FnParam`      | Moved to `lps-shared` |

## Dependents to keep in mind

**Currently use `lps-shared`:**

- `lps-frontend` — for logical types when lowering
- `lps-filetests` — for test value types
- `lpvm` — re-exports and runtime data
- `lps-exec` — depends on `lps-shared` and `lpvm`

**Will eventually use new LPVM traits (M5-M6):**

- `lp-engine`, `lps-filetests`, `lps-exec` (replacement)

## Done When

- [x] `lps-shared` exists with `Lps*` logical types
- [x] `lpvm` exists, `no_std + alloc`, depends on `lps-shared`
- [x] `LpvmData`, `VmContext`, layout helpers present
- [x] Workspace and embedded checks pass
- [x] `lps-frontend`, `lps-filetests` updated to use `lps-shared`

## Post-M1 Notes

The naming differs from the original plan in these ways:

1. **Logical value types use `Lps*` prefix** — `LpsValue`, `LpsType` live in
   `lps-shared` because they're shader/language concepts, not VM runtime concepts.

2. **Runtime data uses `Lpvm*` prefix** — `LpvmData` is a byte buffer for VM
   memory; it's distinct from the logical `LpsValue`.

3. **VMContext kept short name** — `VmContext` (not `LpvmVmContext`) since
   it's already clearly a VM concept and the constant is widely used.

4. **Crate stayed in `lp-shader/`** — moving to repo root was deferred; the
   important part is the clean dependency graph, not the directory path.

The deferred items (`LpvmModule` traits, metadata structs, `LpvmError`) will be
addressed in M2-M4 as the WASM and Cranelift backends are built.
