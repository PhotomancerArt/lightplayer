# Design — `lpir-inliner` stage i (M0 stable `CalleeRef`)

## Scope of work

Replace flat `CalleeRef(u32)` with `CalleeRef::Import(ImportId)` / `CalleeRef::Local(FuncId)`, store local functions in `BTreeMap<FuncId, IrFunction>` with stable ids (no redundant `func_id` on `IrFunction`), keep `imports: Vec<ImportDecl>` with `ImportId` = vector index. Update all `lpir` and downstream crates. **No intentional semantic change**; validate with full test matrix from M0 roadmap.

See `00-notes.md` for resolved planning questions.

## Implementation granularity

Intermediate phases **do not need to keep the workspace building**. It is fine if `cargo check` fails after an early phase until downstream crates are updated. The **contract is end-to-end green** after phase **5** (full test matrix + firmware `cargo check` in `05-cleanup-and-validation.md`). Phases are organizational slices, not merge checkpoints.

## File structure (relevant areas)

```
lp-shader/lpir/src/
├── types.rs                    # UPDATE: ImportId, FuncId, CalleeRef enum
├── lpir_module.rs              # UPDATE: BTreeMap functions; import helpers
├── builder.rs                  # UPDATE: ModuleBuilder next_func_id; add_* returns
├── lpir_op.rs                  # (Call shape unchanged; CalleeRef type only)
├── print.rs                    # UPDATE: callee + function iteration
├── parse.rs                    # UPDATE: CalleeRef construction
├── validate.rs                 # UPDATE: local lookup by FuncId
├── interp.rs                   # UPDATE: callee resolution + callee body fetch
├── lib.rs                      # UPDATE: re-export ImportId, FuncId
└── tests/                      # UPDATE: CalleeRef construction

lp-shader/lpvm-native/src/
├── lower.rs                    # UPDATE: resolve_callee_name, sret path
├── compile.rs, link.rs         # UPDATE: iterate functions / indices
├── regalloc/render.rs          # UPDATE: comment / clone path for map
├── debug_asm.rs, rt_emu/*.rs, rt_jit/*.rs, …  # UPDATE: ir.functions access

lp-shader/lpvm-wasm/src/
├── emit/mod.rs, emit/imports.rs, emit/ops.rs
├── compile.rs                  # zip IR funcs with meta — order contract
└── rt_*/instance.rs

lp-shader/lpvm-cranelift/src/
└── module_lower.rs, emit/call.rs, call.rs, …  # UPDATE: index→FuncId; alias cranelift FuncId

lp-shader/lps-frontend/src/
├── lower.rs, lower_ctx.rs, lower_lpfx.rs

lp-shader/lpvm-emu/src/
└── instance.rs, emu_run.rs

lp-shader/lpvm/src/debug.rs     # (verify; may be HashMap name→, not LpirModule)

lp-shader/lps-filetests, …      # indirect via frontend
```

## Conceptual architecture

```
┌─────────────────────────────────────────────────────────────┐
│ LpirModule                                                   │
│   imports: Vec<ImportDecl>     ImportId(i) ↔ imports[i]      │
│   functions: BTreeMap<FuncId, IrFunction>  (stable keys)     │
└─────────────────────────────────────────────────────────────┘
              │
              │  CalleeRef::Import(id) ──► ImportDecl + index in imports
              │  CalleeRef::Local(id)  ──► functions.get(&id)
              ▼
┌─────────────────────────────────────────────────────────────┐
│ ModuleBuilder                                                │
│   next_func_id: u16 (or u32) monotonic for new locals       │
│   add_function → insert map, return Local(FuncId)            │
└─────────────────────────────────────────────────────────────┘
```

**Id allocation:** each `add_function` allocates the next unused `FuncId` (wrapper type over incrementing counter). **Deletion** is out of scope for M0, but the map + stable ids is the intended contract for M5.

**Name collision:** Cranelift uses `cranelift_module::FuncId`; LPIR gains `lpir::FuncId`. Use explicit qualification or `use lpir::FuncId as LpirFuncId` in files where both appear.

## Main components and interactions

| Component | Role |
|-----------|------|
| `ImportId` / `FuncId` | Newtype wrappers (`u16`); `Hash`, `Ord` for map keys |
| `CalleeRef` | Enum; all `Call` and name resolution match on it |
| `LpirModule::callee_as_*` | Becomes `callee_as_import` → `Option<ImportId>` + slice access, or match-only helpers; local path returns `Option<&IrFunction>` via `FuncId` |
| `ModuleBuilder` | Owns `next_func_id`; `finish()` moves map into `LpirModule` |
| Backends | Replace `functions[i]` / `enumerate()` with map iteration or sorted `Vec<FuncId>` for deterministic codegen order matching existing behavior |

## Suggested implementation phases

Listed as separate files `01-*.md` … `05-*.md` in this directory.

1. **LPIR core** — types, `LpirModule`, `ModuleBuilder`, `lib` exports; compile `lpir` only.
2. **LPIR surface** — print, parse, validate, interp, unit tests.
3. **Primary backends** — `lpvm-native`, `lpvm-wasm`, `lps-frontend` (+ `lower` paths).
4. **Remaining runtimes** — `lpvm-cranelift` (index/order maps; `FuncId` alias), `lpvm-emu`, JIT/EMU instances, `link.rs` / `compile.rs` ordering vs `LpsModuleSig`.
5. **Cleanup & validation** — `cargo test` / `cargo check` matrix from M0, fix warnings, `summary.md`, move plan to `docs/plans-done/` when done.

## Validate (full stage)

From M0 roadmap (run from workspace root):

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-wasm
cargo test -p lps-frontend
cargo test -p lps-filetests -- --test-threads=4
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

Add `cargo test -p lpvm-cranelift` / `cargo test -p lpvm-emu` if those crates cover changed paths.
