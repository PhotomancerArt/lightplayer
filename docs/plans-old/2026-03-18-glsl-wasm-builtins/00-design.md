# WASM builtin calling — design

Parent roadmap: `docs/roadmaps/2026-03-13-glsl-wasm-playground/`  
Related: `docs/plans/2026-03-17-glsl-wasm-part-iii/` (phases 6–9 superseded here for builtins /
linking / validation)

Question log: `00-notes.md`

## Scope

- **Shader codegen:** import section for used builtins; optional memory import for shared linear
  memory; correct WASM function indices (imports first, then user functions).
- **Builtin resolution:** auto-generated `glsl_to_builtin_id` (and related glue); inline compound
  builtins (match Cranelift); scalar imports + component-wise vectors.
- **`builtins.wasm`:** thin `wasm32-unknown-unknown` crate linking `lps-builtins`, built by
  `build-builtins.sh`, **imported memory** (`--import-memory` or equivalent).
- **Runtime:** host-owned `Memory`; instantiate `builtins.wasm` then shader with the same memory and
  builtins exports — **same path** in wasmtime tests and browser.
- **LPFX / out params:** pointers into shared memory; bump or static offsets for out slots.
- **Validation:** builtins filetests, rainbow.shader, docs, cleanup.

Non-goals for this plan: browser UI (playground shell is separate); matrix builtins.

## Decisions (summary)

| Topic                 | Decision                                                                                                             |
|-----------------------|----------------------------------------------------------------------------------------------------------------------|
| Import discovery      | Pre-scan AST for builtin / LPFX calls before codegen                                                                 |
| Vector builtins       | Match Cranelift: component-wise scalar imports; compound builtins inline                                             |
| Import module/name    | `"builtins"` / `BuiltinId::name()`                                                                                   |
| GLSL → BuiltinId      | Auto-generated: `glsl_q32_math_builtin_id`, `glsl_lpfn_q32_builtin_id`, `GlslParamKind` in `glsl_builtin_mapping.rs` |
| Out params / sret     | Offsets in shared linear memory                                                                                      |
| Test vs prod builtins | Same `builtins.wasm` + shared memory (no divergent native-only path)                                                 |
| Memory ownership      | Host creates memory; shader and builtins **import** it (future: textures use same region)                            |
| Q32 mul/div           | Fixed with temp local before i64 extend (remove `#[ignore]` on mul test when touched)                                |

## File structure

```
lp-shader/
├── lps-builtin-ids/
│   └── src/
│       └── lib.rs                         # UPDATE: generated glsl_to_builtin_id (etc.)
├── lps-builtins-gen-app/              # UPDATE: emit new generated helpers
├── lps-builtins-wasm/                 # NEW: wasm32 crate → builtins.wasm, import memory
├── lps-wasm/
│   └── src/
│       ├── codegen/
│       │   ├── mod.rs                     # UPDATE: sections order, pre-scan, indices
│       │   ├── builtin_scan.rs          # NEW: collect used builtins / LPFX
│       │   ├── imports.rs               # NEW: import entries, BuiltinId → index
│       │   ├── memory.rs                # NEW: optional bump / out slots (offsets)
│       │   └── expr/
│       │       ├── mod.rs               # UPDATE: FunCall → builtins
│       │       └── builtins/            # NEW: inline + import dispatch
│       ├── module.rs
│       └── lib.rs
├── lps-filetests/
│   └── src/test_run/
│       ├── wasm_runner.rs               # UPDATE: memory + builtins.wasm + linker
│       └── wasm_link_builtins.rs        # NEW (optional): shared link helper / codegen hook
scripts/
└── build-builtins.sh                    # UPDATE: build builtins.wasm
```

Generated artifacts (paths TBD in implementation):

- `builtins.wasm` (output of `lps-builtins-wasm`)
- Optional: generated wasmtime linker glue next to existing generated builtin refs

## Architecture

```
                    ┌──────────────────┐
                    │ Host: Memory     │
                    │ (single linear)  │
                    └────────┬─────────┘
                             │ import "env" "memory"
              ┌──────────────┴──────────────┐
              ▼                             ▼
     ┌─────────────────┐           ┌─────────────────┐
     │ builtins.wasm   │           │ shader.wasm     │
     │ exports funcs   │           │ imports funcs   │
     │ imports memory  │           │ imports memory  │
     └────────┬────────┘           └────────┬────────┘
              │                             │
              └──────────┬──────────────────┘
                         │ "builtins" module:
                         │ shader imports call → builtins exports
```

**Codegen pipeline**

1. Parse/analyze GLSL → `TypedShader`.
2. **Pre-scan** all function bodies → `HashSet<BuiltinId>` (or names + resolution pass).
3. Sort builtins deterministically → import indices `0..M-1`.
4. Emit **import section** (functions + memory import if needed).
5. Emit types / func / export / code for user functions with **base index = M** (imports first).
6. FunCall: if inline builtin → emit IR; else `glsl_to_builtin_id` → `call` import index.

**Instantiation (wasmtime / browser)**

1. Allocate `Memory` (or `WebAssembly.Memory`).
2. Instantiate `builtins.wasm` with `{ env: { memory } }`.
3. Build import object for shader:
   `{ env: { memory }, builtins: <exports from builtins instance> }`.
4. Instantiate `shader.wasm`.

## Main components

| Component                        | Role                                                          |
|----------------------------------|---------------------------------------------------------------|
| `builtin_scan`                   | AST walk; records which `BuiltinId` / imports are needed      |
| `imports`                        | WASM import section: builtins funcs + memory; stable ordering |
| `glsl_to_builtin_id` (generated) | Map GLSL name + arg count → `Option<BuiltinId>`               |
| `codegen/expr/builtins`          | Inline set (clamp, mix, …) vs `call` to import                |
| `lps-builtins-wasm`          | Produces `builtins.wasm` with imported memory                 |
| `wasm_runner`                    | Loads `builtins.wasm`, wires memory + exports, runs shader    |

## Phases

| # | File                               | Title                                            |
|---|------------------------------------|--------------------------------------------------|
| 1 | `01-builtins-wasm-artifact.md`     | `builtins.wasm` crate + import-memory            |
| 2 | `02-generated-mappings.md`         | Generated `glsl_to_builtin_id` + generator       |
| 3 | `03-shader-imports-and-memory.md`  | Pre-scan, import section, memory import, indices |
| 4 | `04-builtin-codegen.md`            | Inline vs import, FunCall                        |
| 5 | `05-wasmtime-linking.md`           | Wasmtime + shared memory + `builtins.wasm`       |
| 6 | `06-lpfn-out-params.md`            | LPFX, out params, memory slots                   |
| 7 | `07-rainbow-validation-cleanup.md` | Rainbow, filetests, docs, plan done              |

## Validate (repo-wide)

- `cargo build` / `cargo test` for touched crates
- `scripts/glsl-filetests.sh --target wasm.q32` (builtins directories as they unlock)
- `cargo +nightly fmt`
- `just build-fw-esp32` if workspace policy requires
