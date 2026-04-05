# Stage V2: Filetest integration — design

## Scope of work

Wire **`lp-glsl-filetests`** to **`lpir-cranelift`** for **`jit.q32`** and
**`rv32.q32`**, keep **`wasm.q32`**, **remove legacy `cranelift.q32`**, depend on
the **new shared crates** for **`GlslExecutable`** / **`GlslValue`** (see below),
and set **default targets + CI matrix** per `00-notes.md`.

**Legacy crates** (`lp-glsl-frontend`, `lp-glsl-cranelift`, …) stay **unchanged**
for V2; they keep their own copies until a later deprecation/removal pass.

## File structure

```
lp-shader/lp-glsl-diagnostics/   # DONE: ErrorCode, GlslError, GlSourceLoc, …
lp-shader/lps-shared/          # DONE: Type, StructId, FunctionSignature (no registry)
lp-shader/lpvm/        # DONE: GlslValue (+ glsl parse dep)
lp-shader/lp-glsl-exec/          # DONE: GlslExecutable trait (no DirectCallInfo; legacy JIT keeps that)

lp-shader/lp-glsl-frontend/      # V2: no hoist — leave as-is until deprecation
lp-shader/lp-glsl-cranelift/     # V2: no edits for filetests — Stage VII: delete crate

lp-shader/lp-glsl-wasm/
└── src/                       # UPDATE: impl GlslExecutable from lp-glsl-exec; GlslValue from lpvm

lp-shader/lp-glsl-filetests/
├── Cargo.toml                 # UPDATE: lpir-cranelift + lp-glsl-exec (+ values/diagnostics); REMOVE lp-glsl-cranelift
├── README.md                  # UPDATE: defaults, CI matrix, --target wasm.q32 | rv32.q32
└── src/
    ├── target/mod.rs          # UPDATE: no Cranelift; DEFAULT_TARGETS = [jit]; ALL_TARGETS
    ├── parse/parse_annotation.rs
    ├── test_run/
    │   ├── compile.rs         # UPDATE: Wasm | Jit | Rv32 only
    │   ├── lpir_jit_executable.rs
    │   └── lpir_rv32_executable.rs
    └── ...
```

## Conceptual architecture

```
                    Test GLSL snippet + Target
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
          wasm.q32       jit.q32         rv32.q32
       (WasmExecutable) (LpirJit…)    (LpirRv32…)
              │               │               │
              └───────────────┴───────────────┘
                            │
                    GlslExecutable (lp-glsl-exec)
                    GlslValue (lpvm)
                            │
                    run_detail / execution
```

- **No legacy Cranelift** in the runner.
- **Trait / value home:** **`lp-glsl-exec`** + **`lpvm`** (+ **`lp-glsl-diagnostics`** for *
  *`GlslError`**). Small dependency order: diagnostics → core → values → exec. Filetests and *
  *`lp-glsl-wasm`** depend on these; they do **not** need **`lp-glsl-cranelift`** for the trait
  boundary.

## Target naming

| `Target::name()` | Backend | isa + exec         | Compiler path        |
|------------------|---------|--------------------|----------------------|
| `wasm.q32`       | `Wasm`  | Wasm32 + Emulator  | `lp-glsl-wasm`       |
| `jit.q32`        | `Jit`   | Native + Jit       | `lpir-cranelift` JIT |
| `rv32.q32`       | `Rv32`  | Riscv32 + Emulator | `lpir-cranelift` V1  |

**`ALL_TARGETS`:** all three (for `from_name` and CI). **`DEFAULT_TARGETS`:**
**`[jit.q32]`** only.

## Main components

| Component                                                      | Role                                                                                    |
|----------------------------------------------------------------|-----------------------------------------------------------------------------------------|
| `lp-glsl-diagnostics` / `lps-shared` / `lpvm` / `lp-glsl-exec` | Shared types (**done**); legacy crates still duplicate until removed later.             |
| `lp-glsl-wasm`                                                 | `impl GlslExecutable for WasmExecutable` using **`lp_glsl_exec`** / **`lpvm`**.         |
| `lpir_jit_executable.rs` / `lpir_rv32_executable.rs`           | `impl GlslExecutable` for lpir paths in filetests (or small sibling crate if we split). |
| `compile.rs`                                                   | `match backend { Wasm => …, Jit => …, Rv32 => … }` only.                                |
| CI / docs                                                      | Run full target list; locals default to `jit.q32`.                                      |

## Phases (see `01-` … `06-` in this directory)

**Prerequisite (done):** **`lp-glsl-diagnostics`**, **`lps-shared`**, **`lpvm`**, *
*`lp-glsl-exec`** are in the workspace; legacy code was **not** refactored—only copies for the new
stack.

**Order:** phase **04** (filetests **`Cargo.toml`** + imports + **`compile_for_target`** + wasm *
*`impl`** rewired to **`lp_glsl_exec`**) should land **before** phases **02–03** (lpir adapters) so
adapters implement the stable trait from **`lp-glsl-exec`**.

1. **01** — Target matrix; remove Cranelift; `DEFAULT_TARGETS = [jit.q32]`;
   `ALL_TARGETS`; annotations `jit` / `rv32` / `wasm`
2. **04** — Wire filetests + wasm to **`lp-glsl-exec`** / **`lpvm`** (and diagnostics as
   needed); drop **`lp-glsl-cranelift`** from filetests; compile dispatch without Cranelift (**do
   not** edit **`lp-glsl-frontend`** / **`lp-glsl-cranelift`** for this unless unavoidable)
3. **02** — `LpirJitExecutable`
4. **03** — `LpirRv32Executable`
5. **05** — Corpus annotation migration, README, CI matrix
6. **06** — Cleanup, summary, plans-done

## Dependencies

- **Stage IV** — `jit`, `JitModule`, `call`, `CompileOptions`.
- **Stage V1** — object, link, emulator for **`rv32.q32`**.

## Interactions with annotations

- `@ignore(backend=jit)` / `rv32` / `wasm` — no `cranelift`.
- Mechanical pass: old `backend=cranelift` → **`jit`** or **`rv32`**.
