# Phase 4: Stack Args + Integration

## Scope

Handle stack-passed arguments (>8 args) in the emitter, run the full GLSL
filetest suite, and add any missing execution tests.

## Implementation

### 1. Emitter: outgoing stack args

In `emit_vinst` for `VInst::Call`: for arg index `i >= 8` (or `>= 7` when
`callee_uses_sret`), emit `sw` from the vreg's allocated location to
`SP + outgoing_offset(i)`.

The allocator already handles these args as normal uses (vreg in pool reg or
spill slot). The emitter reads the allocation and stores to the outgoing area.

Frame layout: `FrameLayout::compute` already accounts for outgoing arg area
size. Wire `max_outgoing_stack_args` to the actual maximum across all calls.

### 2. Emitter: incoming stack args (callee-side)

For functions receiving >8 args: in the prologue, load stack-passed params from
the caller's frame (`FP + incoming_offset(i)`) into their allocated registers.

### 3. LPIR filetest

Add `call/stack_args.lpir`:

```
; import: many_args(i32, i32, i32, i32, i32, i32, i32, i32, i32) -> i32
; abi_params: 1
func @test(v1: i32) -> i32 {
    ...set up 9+ args...
    call @many_args(...)
    ret result
}
```

Verify: first 8 args in ARG_REGS, remaining args have normal Alloc (pool reg or
stack), snapshot shows no ARG_REG for overflow args.

### 4. GLSL integration: run full suite

```bash
# All native call filetests
TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored native
```

Expected to pass:
- `native-call-simple.glsl`
- `native-call-multi-args.glsl`
- `native-call-nested.glsl`
- `native-call-vec2-return.glsl`
- `native-call-vec4-return.glsl`
- `native-call-mat4-return.glsl`
- `native-multi-function.glsl`
- `perf/caller-save-pressure.glsl`
- `perf/stack-args-outgoing.glsl`
- `perf/stack-args-incoming.glsl`
- `perf/stack-args-incoming-16.glsl`
- `perf/nested-call-overhead.glsl`

Out of scope (requires control flow, M4):
- `native-call-control-flow.glsl`
- `perf/live-range-interference.glsl`
- `perf/spill-density.glsl`
- `perf/mat4-reg-pressure.glsl`

### 5. Add missing GLSL filetests

Add alongside existing files in `lps-filetests/filetests/lpvm/native/`:

**`native-call-arg-live-after.glsl`**: Variable used as call arg AND used
after the call.

```glsl
int identity(int x) { return x; }

int test_arg_live_after_call() {
    int a = 42;
    int r = identity(a);
    return a + r;  // a is arg AND live after
}

// run: test_arg_live_after_call() == 84
```

**`native-call-sret-chain.glsl`**: Result of sret call used as arg to
another call.

```glsl
vec4 make_vec(float x) { return vec4(x, x*2.0, x*3.0, x*4.0); }

float test_sret_chain() {
    vec4 v = make_vec(1.0);
    vec4 w = make_vec(v.x + v.y);
    return w.x + w.y + w.z + w.w;
}

// run: test_sret_chain() ~= 30.0
```

**`native-call-sret-stack-args.glsl`**: Sret + >7 user args (overflow at 7).

```glsl
vec4 sret_many(float a, float b, float c, float d,
               float e, float f, float g, float h) {
    return vec4(a+b, c+d, e+f, g+h);
}

float test_sret_stack_args() {
    vec4 r = sret_many(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0);
    return r.x + r.y + r.z + r.w;
}

// run: test_sret_stack_args() ~= 36.0
```

### 6. Final cleanup

- Ensure all trace output is clean and readable
- Update roadmap with completion status
- Run ESP32 build check

## Validation

```bash
# All allocator tests
cargo test -p lpvm-native

# All GLSL native filetests
TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored native

# ESP32 build
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

## Success Criteria

- All LPIR filetests pass (including call/ and stack_args)
- All applicable GLSL filetests pass under rv32fa
- New GLSL filetests (arg-live-after, sret-chain, sret-stack-args) pass
- ESP32 builds
- No regressions
