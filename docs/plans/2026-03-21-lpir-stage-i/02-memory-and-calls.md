# Phase 2: Memory Model, Slots, and Calls

## Scope

Write two spec chapters:
- `docs/lpir/03-memory.md` — Slot declarations, memory ops, pointer
  semantics, use case examples.
- `docs/lpir/05-calls.md` — Function declarations, import vs local
  functions, call op, multi-return.

## Reminders

- This is a spec-writing phase, no Rust code.
- Be precise about addressing, alignment, and offset semantics.
- Document the mapping to both WASM and Cranelift for each concept.

## Implementation details

### 1. Memory Model section

#### Pointer semantics

- Pointers are i32 VRegs holding byte addresses. No special pointer type.
- Pointer arithmetic uses regular `iadd` / `imul`.
- All scalar values are 4 bytes (f32, i32). Bool is stored as i32 (0/1)
  in memory.
- Alignment defaults to natural (4 bytes). The spec does not expose
  alignment control.

#### Slot declarations

Slots are function-level metadata declaring addressable stack memory.

```
func @example(v0:i32) -> f32 {
  slot ss0, 64              ; 64 bytes
  slot ss1, 16              ; 16 bytes
  ...
}
```

Document:
- Syntax: `slot ssN, <size_bytes>` — declared before the function body ops.
- Naming: `ss0`, `ss1`, ... (per function).
- Size is static, known at compile time.
- Slots are not initialized (contents are undefined until written).
- Slots are function-scoped (lifetime = function invocation).

Target mapping:
- Cranelift: each slot → `create_sized_stack_slot`.
- WASM: all slots summed → shadow stack frame. Prologue subtracts from
  stack pointer global, epilogue restores.

#### Memory ops

```
v1:i32 = slot_addr ss0         ; get base address of slot ss0
                               ; result is i32 (byte address)

v2:f32 = load v1, 0           ; load f32 from address v1 + 0
v3:i32 = load v1, 4           ; load i32 from address v1 + 4
                               ; result type determines load width/type
                               ; offset is a static byte offset (non-negative)

store v1, 0, v4               ; store v4 to address v1 + 0
store v1, 4, v5               ; store v5 to address v1 + 4
                               ; value type determines store width/type
                               ; offset is a static byte offset (non-negative)

memcpy v_dst, v_src, 64       ; copy 64 bytes from v_src to v_dst
                               ; size is a static byte count
                               ; v_dst and v_src are i32 addresses
```

Semantics notes for each op:
- `slot_addr`: returns an i32 address. The address is valid for the duration
  of the function invocation. Behavior is undefined if used after return.
- `load`: reads a scalar value from `base + offset`. The loaded type is
  determined by the result VReg's type annotation. Offset is an unsigned
  literal (not a VReg). Unaligned access behavior is target-defined.
- `store`: writes a scalar value to `base + offset`. The stored type is
  determined by the value VReg's type. Offset is an unsigned literal.
- `memcpy`: copies `size` bytes from `src` to `dst`. Both are i32 addresses.
  Size is an unsigned literal. Overlapping regions: behavior is undefined
  (use only for non-overlapping copies).

Target mapping:
- `load`/`store` → WASM `i32.load`/`i32.store`/`f32.load`/`f32.store`
  with MemArg `{ offset, align: 2 }`.
- `load`/`store` → Cranelift `load`/`store` with MemFlags.
- `memcpy` → WASM `memory.copy`.
- `memcpy` → Cranelift `emit_small_memory_copy` or `call_memcpy`.

#### Use case examples

Include examples covering all memory use cases:

1. **LPFX out-pointer ABI**:
```
func @noise_example(v0:f32, v1:f32) -> f32 {
  slot ss0, 12                        ; scratch for vec3 result
  v2:i32 = slot_addr ss0
  v3:i32 = ftoi_sat_s v0             ; flatten to i32 for Q32
  v4:i32 = ftoi_sat_s v1
  store v2, 0, v3                    ; prepend result pointer (implicit)
  call @__lpfx_noise3(v2, v3, v4)
  v5:f32 = load v2, 0               ; result.x
  v6:f32 = load v2, 4               ; result.y
  v7:f32 = load v2, 8               ; result.z
  return v5
}
```

2. **Out/inout parameter**:
```
func @compute(v0:f32, v1:i32) {      ; v1 = out ptr for vec3
  v2:f32 = fmul v0, v0
  store v1, 0, v2                    ; out.x
  store v1, 4, v2                    ; out.y
  store v1, 8, v2                    ; out.z
}
```

3. **Local array with dynamic indexing**:
```
func @arr_example(v0:i32) -> f32 {
  slot ss0, 16                        ; float[4]
  v1:i32 = slot_addr ss0
  v2:f32 = fconst.f32 1.0
  store v1, 0, v2
  store v1, 4, v2
  store v1, 8, v2
  store v1, 12, v2
  v4:i32 = imul_imm v0, 4              ; offset = index * 4
  v5:i32 = iadd v1, v4              ; address = base + offset
  v6:f32 = load v5, 0               ; arr[v0]
  return v6
}
```

4. **Globals via context pointer**:
```
func @shader(v0:f32, v1:i32) -> f32 { ; v1 = context ptr
  v2:f32 = load v1, 0                 ; ctx.global_a
  v3:f32 = load v1, 4                 ; ctx.global_b
  v4:f32 = fadd v2, v3
  store v1, 0, v4                     ; write back global_a
  return v4
}
```

5. **Bulk copy (matrix)**:
```
func @mat_copy(v0:i32, v1:i32) {     ; dst ptr, src ptr
  memcpy v0, v1, 64                  ; copy mat4 (16 floats)
}
```

### 2. Function Declarations and Call Conventions section

#### Function declarations

Three kinds:
- **Imported functions**: `import @name(param_types) -> return_type`
- **Local functions**: `func @name(params) -> return_type { body }`
- **Exported functions**: `export func @name(params) -> return_type { body }`

`export` marks a function as callable from the host (WASM export, Cranelift
entry point). Most modules have one export (the shader entry). This is an
IR-level concept — the emitter doesn't need out-of-band configuration to
know which functions to export.

```
import @__lp_q32_add(i32, i32) -> i32
import @__lpfx_noise3(i32, i32, i32, i32) -> (i32, i32, i32)

export func @shader_main(v0:i32) -> f32 {
  ...
}

func @smoothstep(v0:f32, v1:f32, v2:f32) -> f32 {
  ...
}

func @vec3_return(v0:f32) -> (f32, f32, f32) {
  ...
}

func @void_func(v0:f32, v1:i32) {
  ...
}
```

Document:
- Import declarations specify parameter types (no VReg names).
- Local function declarations specify parameter VRegs with types.
- Return type is optional (omit for void functions).
- Multiple return values are supported: `-> (f32, f32, f32)`.
- Functions have a name prefixed with `@`.

#### Call op

```
v5:f32 = call @my_helper(v1, v2)              ; single return value
call @void_func(v1, v2)                       ; void (no return)
v5:f32, v6:f32, v7:f32 = call @vec3_fn(v1)   ; multi-return (scalarized vec3)
```

Document:
- Single `call` op for both imported and local functions.
- The emitter uses the function declaration to determine linkage.
- Arguments are VRegs, passed by value.
- Return value(s) bound to destination VReg(s). Multiple returns for
  scalarized vector/matrix results.
- For imported functions, the lowering is responsible for ABI translation
  (flattening args to i32, prepending out-pointers, post-call loads).

Multi-return target mapping:
- WASM: native multi-value (core spec since 2020).
- Cranelift: multi-return for small counts, automatic StructReturn
  (pointer-based) when the ABI doesn't support enough return registers.
  The emitter pushes multiple `AbiParam` returns; Cranelift handles the
  rest.

## Validate

Review the section for:
- Memory ops have complete syntax, operand types, and semantics.
- Slot declarations are clearly separated from runtime ops.
- All use cases (LPFX, out params, arrays, globals, bulk copy) have examples.
- Call conventions are clear for both import and local.
- Target mappings (WASM, Cranelift) are documented for each concept.
