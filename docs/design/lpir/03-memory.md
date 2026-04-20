# Memory Model

This chapter defines pointer representation, function slots, memory operations, and the assumptions lowering must satisfy for addressing.

## Pointer semantics

- **Byte addresses** for memory operations use the `ptr` type (`ptr` in text, `IrType::Pointer` in code). VM context, `slot_addr` results, GLSL `out` / `inout` formal bases, and similar values are `ptr`.
- **Offsets and indices** remain `i32` (for example stride × index via `imul`, then added to a `ptr` base). Emitters treat address arithmetic as pointer-sized where the base is `ptr`.
- Scalar `load` and `store` use natural alignment: 4 bytes for `f32` and `i32`. The specification does not expose alignment attributes to the author. Behavior of accesses that are not naturally aligned is target-defined.
- Scalar loads and stores use little-endian byte order in memory. WebAssembly requires this for its memory instructions; typical embedded targets used with this stack match that ordering.

## Slot declarations

Slots declare function-scoped, addressable stack storage. They appear in the function header alongside parameters, before the executable operations.

```
func @example(v1:f32) -> f32 {
  slot ss0, 64
  slot ss1, 16
  ...
}
```

(`v0:ptr` is implicit VM context; the first user parameter is `v1` in the text form.)

| Item | Rule |
|------|------|
| Syntax | `slot <name>, <size_bytes>` |
| Name | Per-function identifiers such as `ss0`, `ss1`, … |
| Size | Non-negative integer literal; fixed at compile time |
| Initial value | Undefined until a `store` or `memcpy` writes the bytes |
| Lifetime | One function activation; invalid after `return` |

### Target mapping

- **Cranelift:** Each slot corresponds to a sized stack slot (for example `create_sized_stack_slot`); the emitter places `slot_addr` relative to the frame.
- **WebAssembly:** Shadow stack with frame elision. A mutable `i32` global (conventionally `$sp`) holds the current stack pointer in linear memory; LPIR still types those addresses as `ptr`, and the WASM emitter maps `ptr` to `i32`. Functions that declare at least one slot emit a prologue that reserves space (decrement `$sp` by the frame size) and an epilogue that restores `$sp`. Functions with no slots omit both prologue and epilogue. Scratch storage for LPFX and similar uses the same slot mechanism; there is no separate global scratch region in LPIR.

## Memory operations

```
v1:ptr = slot_addr ss0
v2:f32 = load v1, 0
store v1, 0, v4
memcpy v_dst, v_src, 64
```

### `slot_addr`

| | |
|--|--|
| Syntax | `v:ptr = slot_addr <slot_name>` |
| Operands | Named slot declared in the enclosing function |
| Result | `ptr` byte address of the slot’s first byte |
| Semantics | The value is valid from the slot’s allocation in the prologue until the function returns. Using it after return is invalid. |

### `load`

| | |
|--|--|
| Syntax | `v_result:T = load v_base, <offset>` |
| Operands | `v_base` — `ptr`; `<offset>` — unsigned integer literal (byte displacement) |
| Result | Scalar `T` (`f32` or `i32`); width and interpretation follow the result VReg type |
| Semantics | Reads `sizeof(T)` bytes from `v_base + offset`. The offset is not a VReg. |

### `store`

| | |
|--|--|
| Syntax | `store v_base, <offset>, v_value` |
| Operands | `v_base` — `ptr`; `<offset>` — unsigned integer literal; `v_value` — scalar `f32` or `i32` |
| Result | None |
| Semantics | Writes `sizeof(type(v_value))` bytes to `v_base + offset`. |

### `memcpy`

| | |
|--|--|
| Syntax | `memcpy v_dst, v_src, <size>` |
| Operands | `v_dst`, `v_src` — `ptr`; `<size>` — non-negative integer literal (byte count) |
| Result | None |
| Semantics | Copies `size` bytes from `v_src` to `v_dst`. The source and destination ranges must not overlap (same contract as C `memcpy`). Overlapping regions require a different lowering strategy (temporary buffer or byte-wise loop), not this opcode as specified. |

### Target mapping (summary)

- **WebAssembly:** `load` / `store` lower to typed memory instructions with the literal folded into `offset` in the memory immediate where applicable; `memcpy` may use `memory.copy` when the target and overlap rules allow.
- **Cranelift:** `load` / `store` with `MemFlags`; `memcpy` via small inline copies or a libc-style memcpy call, per emitter policy.

## Dynamic indexing

The effective address operand of `load` and `store` is a `ptr` VReg (for example `slot_addr` plus index × stride folded via `iadd` on a `ptr` base and `i32` offset). The second operand remains a compile-time byte offset, often `0` when all displacement is folded into the base.

## `out` and `inout` parameters

GLSL/Naga pointer parameters lower to **`ptr`** user parameters (alongside implicit `v0`). Reads and writes use `load` and `store`. The caller passes a `ptr` that refers to storage with appropriate size and lifetime for the callee’s accesses.

## Well-formed memory

Well-formed LPIR assumes every `load`, `store`, and `memcpy` touches only bytes that belong to the object being accessed. Lowering that introduces dynamic indexing is responsible for bounds checks or proofs of static safety. LPIR does not define out-of-bounds behavior; a violating program is ill-formed relative to this assumption (a concrete target may trap, fault, or read stale data).

## Examples

### 1. LPFX out-pointer ABI (scratch slot and `noise3`)

The callee writes a `vec3`-sized result through the first argument. A slot supplies the scratch buffer; `slot_addr` yields the pointer passed to the import.

```
func @noise_sample(v1:f32, v2:f32, v3:f32) -> f32 {
  slot ss0, 12
  v_ptr:ptr = slot_addr ss0
  call @lpfn::noise3(v_ptr, v1, v2, v3)
  v_rx:f32 = load v_ptr, 0
  v_ry:f32 = load v_ptr, 4
  v_rz:f32 = load v_ptr, 8
  return v_rx
}
```

### 2. Out / inout parameter (stores through pointer argument)

`v2` is the byte address (`ptr`) of a `vec3` the callee fills.

```
func @fill_vec3(v1:f32, v2:ptr) {
  v3:f32 = fmul v1, v1
  store v2, 0, v3
  store v2, 4, v3
  store v2, 8, v3
}
```

### 3. Local array with dynamic indexing

Four `f32` elements in a slot; index `v1` selects the element. Index times stride is folded into the base; `load` uses offset `0`.

```
func @arr_dyn(v1:i32) -> f32 {
  slot ss0, 16
  v_base:ptr = slot_addr ss0
  v_one:f32 = fconst.f32 1.0
  store v_base, 0, v_one
  store v_base, 4, v_one
  store v_base, 8, v_one
  store v_base, 12, v_one
  v_off:i32 = imul_imm v1, 4
  v_addr:ptr = iadd v_base, v_off
  v_elt:f32 = load v_addr, 0
  return v_elt
}
```

### 4. Globals via context pointer

`v_ctx` points to a struct in memory; fixed offsets name fields.

```
func @use_ctx(v1:f32, v_ctx:ptr) -> f32 {
  v_a:f32 = load v_ctx, 0
  v_b:f32 = load v_ctx, 4
  v_sum:f32 = fadd v_a, v_b
  store v_ctx, 0, v_sum
  return v_sum
}
```

### 5. Bulk copy (`mat4`)

Sixty-four bytes copy (sixteen `f32`). Source and destination ranges must not overlap.

```
func @copy_mat4(v_dst:ptr, v_src:ptr) {
  memcpy v_dst, v_src, 64
}
```
