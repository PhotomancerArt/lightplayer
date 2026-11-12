# Phase 8: Validation with Filetests

## Scope

Run `debug1.glsl` and `native-rv32-iadd.glsl` with new pipeline and fix any issues.

## Steps

### 1. Run filetest with fast backend

```bash
cargo run -p lp-cli -- shader-rv32 --backend fast tests/shaders/debug1.glsl --show-vinst --show-physinst --disassemble 2>&1
```

### 2. Expected output for simple case

Input (GLSL):
```glsl
void main() {
    int a = 10;
    int b = 20;
    int c = a + b;
}
```

Expected VInst:
```
i0 = IConst32 10
i1 = IConst32 20
i2 = Add32 i0, i1
Ret i2
```

Expected PhysInst:
```
FrameSetup 0
li a0, 10
li a1, 20
add a0, a0, a1
ret
FrameTeardown 0
```

### 3. Fix issues as they arise

Common issues to watch for:

1. **Spill slot tracking**: Need to track which VReg spilled to which slot
2. **Rematerialization**: IConst32 should not need a slot, recompute
3. **Call clobbers**: Caller-saved regs need to be spilled/reloaded
4. **Frame layout**: Spill slot calculation and addressing

### 4. Update filetest expectations

If tests pass but output differs slightly (due to different allocation), update expected output files in `tests/shaders/*.glsl.expected`.

### 5. Integration with lp-engine

Update `lp-engine` to use new pipeline when `RegAllocAlgorithm::Fast` is selected:

```rust
// In lp-engine/src/gfx/cranelift.rs or similar
use lpvm_native::isa::rv32fa::{alloc, emit, debug::physinst};

fn compile_fast(fn_body: Lowered) -> Result<Vec<u8>, NativeError> {
    let physinsts = alloc::allocate(&fn_body.vinsts, fn_body.is_sret, ...)?;
    let mut emitter = emit::PhysEmitter::new();
    for inst in &physinsts {
        emitter.emit(inst);
    }
    Ok(emitter.finish())
}
```

## Validate

```bash
cargo test -p lp-cli -- shader_rv32
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
