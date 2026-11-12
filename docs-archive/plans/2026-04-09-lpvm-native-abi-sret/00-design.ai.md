# Design: lpvm-native Sret ABI Implementation

## Scope of Work

Implement the RISC-V RV32 sret (struct-return) calling convention for functions returning >4 scalars:

- mat4 (16 scalars), large structs, multiple vec4s
- Caller allocates buffer, passes pointer in a0
- Real args shifted to a1-a7
- Callee stores return values to buffer

## File Structure

```
lp-shader/lpvm-native/src/
├── lib.rs                      # UPDATE: Re-export AbiInfo
├── error.rs                    # (no change)
├── regalloc/                   # (no change - spill support already done)
├── isa/rv32/
│   ├── abi.rs                  # UPDATE: Add AbiInfo struct, from_lps_sig()
│   ├── emit.rs                 # UPDATE: Thread LpsFnSig, handle Sret in Ret emission
│   └── mod.rs                  # (no change)
├── rt_emu/
│   ├── engine.rs               # (no change)
│   ├── instance.rs             # UPDATE: sret buffer alloc, arg shifting, readback
│   └── module.rs               # UPDATE: Expose signatures for emission
├── debug_asm.rs                # UPDATE: Thread signature info
└── vinst.rs                    # (no change)
```

## Conceptual Architecture

### Caller Side (`rt_emu/instance.rs`)

```
call_q32(name, args):
    1. Lookup LpsFnSig for name
    2. Classify return: ReturnClass::from_lps_sig(sig)
    3. If Sret:
       a. Allocate buffer from arena (size = scalar_count * 4)
       b. Prepend buffer ptr to args (now a0)
       c. Shift all real args (now a1-a7)
       d. Set flag: sret_active = true
    4. Invoke function with modified args
    5. If Sret:
       a. Read return values from buffer
       b. Convert to LpsValueF32
    6. Return result
```

### Callee Side (`isa/rv32/emit.rs`)

```
emit_function_bytes(ir_func, sig):
    1. Classify return: ReturnClass::from_lps_sig(sig)
    2. Lower ops to vinsts
    3. Allocate registers (greedy)
    4. Create frame layout with spill slots
    5. Emit prologue
    6. For each vinst:
       - VInst::Ret { vals }:
         If Sret:
           For each val:
             - Load val to temp (if spilled)
             - Emit: sw temp, offset(a0)  # store to sret buffer
         If Direct:
           For each val:
             - Load val to temp (if spilled)
             - Emit: mv REG[i], temp  # move to a0-a3
    7. Emit epilogue
```

### ABI Classification (`isa/rv32/abi.rs`)

```rust
pub struct AbiInfo {
    pub return_class: ReturnClass,
    pub arg_regs: Vec<PhysReg>,      // Which regs for args (a0-a7, shifted for sret)
    pub sret_size: Option<u32>,      // Buffer size if sret
}

impl AbiInfo {
    pub fn from_lps_sig(sig: &LpsFnSig) -> Self {
        let return_class = ReturnClass::from_lps_types(&[sig.return_type]);
        let arg_regs = match return_class {
            ReturnClass::Sret { .. } => ARG_REGS[1..].to_vec(), // Skip a0
            ReturnClass::Direct { .. } => ARG_REGS.to_vec(),
        };
        let sret_size = match return_class {
            ReturnClass::Sret { .. } => Some(scalar_count(sig.return_type) * 4),
            _ => None,
        };
        Self { return_class, arg_regs, sret_size }
    }
}
```

### Key Changes

1. **abi.rs**: Add `AbiInfo::from_lps_sig()` helper
2. **emit.rs**:
   - Add `sig` parameter to `emit_function_bytes()`
   - Use `AbiInfo` to determine Ret emission strategy
   - For Sret: emit `sw` to a0-relative offsets instead of register moves
3. **instance.rs**:
   - Classify return before call
   - Allocate sret buffer from arena
   - Prepend buffer ptr to args
   - Read return values from buffer after call
4. **module.rs**: Expose signature lookup for emission context

### Memory Layout (Sret Buffer)

```
Buffer layout for mat4 return (16 scalars = 64 bytes):
  [0..3]   = row 0, col 0 (f32 as i32 bits)
  [4..7]   = row 0, col 1
  ...
  [60..63] = row 3, col 3

Callee stores with:
  sw vreg0, 0(a0)
  sw vreg1, 4(a0)
  ...
  sw vreg15, 60(a0)
```

## Main Components and How They Interact

1. **AbiInfo** (abi.rs): Per-function ABI classification from LpsFnSig. Used by both caller and emission.

2. **Sret Detection** (instance.rs): Before call, classify return type. If sret, allocate buffer and adjust arg layout.

3. **Arg Shifting** (instance.rs): For sret functions, buffer ptr goes in a0, real args shift to a1+. Same pattern as Cranelift.

4. **Sret Emission** (emit.rs): VInst::Ret stores to a0-relative buffer offsets instead of moving to a0-a3.

5. **Buffer Management** (instance.rs): Arena allocation for sret buffers (same as VMContext), freed after call.

## Testing Strategy

**Unit tests** (abi.rs):

- `abi_info_mat4_is_sret()` - mat4 → Sret with 64-byte size
- `abi_info_vec4_is_direct()` - vec4 → Direct
- `abi_info_args_shifted_for_sret()` - real args start at a1

**Filetests**:

- `spill_pressure.glsl` - mat4 return, 16 scalars
- Add `mat/mat4_return.glsl` - dedicated mat4 return test

**Validation**:

```bash
cargo test -p lpvm-native abi::tests
cargo test -p lpvm-native --lib
scripts/filetests.sh --target rv32lp.q32 scalar/spill_pressure.glsl
```
