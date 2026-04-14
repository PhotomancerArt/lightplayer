# Phase 3: Sret

## Scope

Support structured return (sret) for functions returning >2 scalar words.
Callee-side first (our function returns via sret buffer), then caller-side
(we call a function that returns sret).

## Implementation

### Callee-side sret

#### 1. Prologue: save sret pointer

In `rv32/emit.rs`, `emit_function` prologue: when `func_abi.is_sret()`, emit
`mv s1, a0` to preserve the sret buffer pointer. The `TODO(M3)` marker at
lines ~192-195 already notes this.

`s1` is already excluded from `ALLOC_POOL` when `is_sret` (done in
`func_abi_rv32`).

#### 2. Ret emission: store to sret buffer

In `emit_vinst` for `VInst::Ret`: when `func_abi.is_sret()`, instead of moving
return values to `a0`/`a1`, emit `sw` instructions storing each return vreg to
`[s1 + i*4]`.

#### 3. Filetest directive: `; abi_return: vec4`

Update filetest parser to accept return types that trigger sret. When
`abi_return` specifies a type with >2 words (vec4 = 4, mat4 = 16), the filetest
constructs `FuncAbi` with `ReturnMethod::Sret`.

#### 4. GLSL validation

```bash
# These already exist, just need to pass under rv32fa
TEST_FILE=native-call-vec4-return TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored
TEST_FILE=native-call-mat4-return TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored
```

### Caller-side sret

#### 5. Allocator: arg shift

In the call processing (Phase 2 step 5), when `VInst::Call.callee_uses_sret`:

```rust
let base = if callee_uses_sret { 1 } else { 0 };
// args[i] → ARG_REGS[base + i]
```

The sret buffer pointer (`a0`) is set up by the emitter, not the allocator.

#### 6. Emitter: sret buffer pointer setup

Before the call instruction, emit `addi a0, sp, sret_slot_base_from_sp` to
point `a0` at the caller's sret buffer.

After the call, results are in the buffer at `[sp + sret_slot_base_from_sp]`.
Emit loads to move them to the vregs' allocated locations.

#### 7. Frame layout: wire sret buffer size

In `emit.rs` / `emit_lowered`, pass `max_callee_sret_bytes` to
`FrameLayout::compute` instead of `0`.

#### 8. LPIR filetest

Add `call/sret_simple.lpir`:

```
; import: big_return(i32) -> vec4
; abi_params: 1
func @test(v1: i32) -> i32 {
    ...call @big_return(v0, v1)...
    ...use results...
    ret
}
```

Verify: `a0` holds sret ptr, vmctx→`a1`, user arg→`a2`.

## Validation

```bash
cargo test -p lpvm-native-fa --test filetests
cargo test -p lpvm-native-fa fa_alloc

# GLSL sret tests
TEST_FILE=native-call-vec4-return TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored
TEST_FILE=native-call-mat4-return TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored
```

## Success Criteria

- Callee-side: functions returning vec4/mat4 write to sret buffer correctly
- Caller-side: arg registers shifted by 1, sret pointer in a0
- GLSL sret filetests pass
- LPIR sret filetest shows correct arg shift in snapshot
