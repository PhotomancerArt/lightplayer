# WASM Builtins: Path to Rainbow — Design

Parent roadmap: `docs/roadmaps/2026-03-13-glsl-wasm-playground/`
Question log: `00-notes.md`

## Scope

- **Fix psrdnoise seed bug:** Add `seed: UInt` to `lpfx_psrdnoise` GLSL signatures (vec2 and vec3
  overloads). Update Cranelift registry, shader sources, generated code. Eliminates UB on native and
  makes WASM import types (already 7 params) correct.
- **Math gaps:** Inline `floor` and `fract` for Q32 in `builtin_inline.rs`. Verify `atan(y,x)` →
  `LpQ32Atan2` mapping works through existing import path.
- **LPFX call emission:** New `lpfx_call.rs` for LPFX FunCall dispatch — arg flattening, out-param
  pointer at memory offset 0, post-call loads.
- **Shared memory for out params:** Static offset 0 (8 bytes) for gradient scratch. Documented in
  `impl-notes.md` with future growth notes.
- **Filetest runner:** Shared `wasm_link.rs` helper in `lp-glsl-filetests` for builtins.wasm +
  memory + linker instantiation.
- **Rainbow end-to-end:** Compile and run `main.glsl` via wasmtime.

Non-goals: browser playground, matrix builtins, memory allocator beyond static offsets.

## Decisions

| Topic            | Decision                                                                    |
|------------------|-----------------------------------------------------------------------------|
| `floor` Q32      | Inline: `i32.shr_s` + `i32.shl` (match Cranelift)                           |
| `fract` Q32      | Inline: `x - floor(x)` (match Cranelift)                                    |
| psrdnoise seed   | Fix bug: add `seed: UInt` to GLSL sig, update Cranelift registry + shaders  |
| LPFX codegen     | New `lpfx_call.rs`, separate from `builtin_call.rs` (match Cranelift split) |
| Out-param memory | Static offset 0, 8 bytes for gradient. No allocator.                        |
| Filetest linking | Shared helper `wasm_link.rs` in `lp-glsl-filetests`                         |

## File structure

```
lp-shader/
├── lp-glsl-frontend/
│   └── src/semantic/lpfx/
│       └── lpfx_fns.rs                          # UPDATE: add seed param to psrdnoise sigs
├── lp-glsl-cranelift/
│   └── src/backend/builtins/
│       └── registry.rs                           # UPDATE: fix psrdnoise sig (add seed param)
├── lp-glsl-wasm/
│   └── src/codegen/
│       ├── mod.rs                                # UPDATE: memory import policy
│       ├── expr/
│       │   ├── mod.rs                            # UPDATE: add is_lpfx_fn dispatch branch
│       │   ├── builtin_inline.rs                 # UPDATE: add floor, fract Q32
│       │   ├── builtin_call.rs                   # (no changes)
│       │   └── lpfx_call.rs                      # NEW: LPFX call emission + arg flatten + out
│       └── memory.rs                             # NEW: out-param offset constants
├── lp-glsl-filetests/
│   └── src/test_run/
│       ├── wasm_runner.rs                        # UPDATE: use wasm_link for builtins+memory
│       └── wasm_link.rs                          # NEW: shared linking helper
├── lp-glsl-builtins-gen-app/                     # UPDATE: regenerate after lpfx_fns.rs change
examples/
├── basic/src/rainbow.shader/main.glsl            # UPDATE: psrdnoise calls add seed arg
├── basic2/src/rainbow.shader/main.glsl           # UPDATE: same
└── mem-profile/src/rainbow.shader/main.glsl      # UPDATE: same
```

## Architecture

```
                    ┌──────────────────┐
                    │ Host: Memory     │
                    │ (single linear)  │
                    │ offset 0-7:      │
                    │   gradient out   │
                    └────────┬─────────┘
                             │ import "env" "memory"
              ┌──────────────┴──────────────┐
              ▼                              ▼
     ┌─────────────────┐           ┌─────────────────┐
     │ builtins.wasm   │           │ shader.wasm     │
     │ exports funcs   │           │ imports funcs   │
     │ imports memory  │           │ imports memory  │
     │ (writes to 0-7  │           │ (reads from 0-7 │
     │  for gradient)  │           │  after psrdnoise│
     └────────┬────────┘           │  call)          │
              │                    └────────┬────────┘
              └──────────┬──────────────────┘
                         │ "builtins" module:
                         │ shader imports → builtins exports
```

## FunCall dispatch

```
FunCall(name, args)
  ├─ scalar/vector constructor  →  constructor.rs
  ├─ user function              →  call via func_index_map
  ├─ inline builtin             →  builtin_inline.rs  (floor, fract, clamp, mix, ...)
  ├─ Q32 math import            →  builtin_call.rs    (sin, cos, exp, atan2, ...)
  ├─ LPFX function              →  lpfx_call.rs       (worley, fbm, psrdnoise)
  └─ error
```

## LPFX call emission (`lpfx_call.rs`)

1. `find_lpfx_fn(name, &arg_types)` → get GLSL signature + `BuiltinId`
2. Look up import index from `ctx.builtin_func_index`
3. For each GLSL param in declaration order:
    - `In` scalar/int/uint → evaluate, push 1 value
    - `In` vec2/vec3/vec4 → evaluate, components already on stack (N values)
    - `Out` vec2 → push `i32.const 0` (memory offset for gradient scratch)
4. `call(func_idx)` — scalar return on stack
5. For `Out` params: `i32.load offset` + `i32.load offset+4` → store to local variables

## Main components

| Component           | Role                                                             |
|---------------------|------------------------------------------------------------------|
| `builtin_inline.rs` | `floor`, `fract` Q32 added to existing inline set                |
| `lpfx_call.rs`      | LPFX dispatch: arg flatten, out-param pointers, post-call loads  |
| `memory.rs`         | Constants for out-param offsets (offset 0 = gradient scratch)    |
| `wasm_link.rs`      | Filetest helper: load builtins.wasm, create memory, build linker |
| `lpfx_fns.rs`       | Fix: add `seed: UInt` to psrdnoise GLSL sigs                     |
| `registry.rs`       | Fix: add seed to Cranelift psrdnoise signatures                  |
