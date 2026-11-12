# ABI2 Integration Design

## Scope of Work

Wire abi2 into the compiler pipeline so it's actually used for:
1. Register allocation (respect ABI constraints)
2. Emission (prologue, sret handling, frame layout)
3. Runtime (sret buffer allocation, argument shifting)

## File Structure

```
lp-shader/lpvm-native/src/
├── abi2/
│   ├── mod.rs                  # (unchanged)
│   ├── regset.rs               # (unchanged)
│   ├── classify.rs             # (unchanged)
│   ├── func_abi.rs             # UPDATE: add helper methods
│   └── frame.rs                # UPDATE: use func_abi.stack_alignment()
├── regalloc/
│   ├── mod.rs                  # UPDATE: thread FuncAbi through
│   └── greedy.rs               # UPDATE: respect precolors, allocatable
├── isa/rv32/
│   ├── abi2.rs                 # UPDATE: add stack_alignment() helper
│   ├── emit.rs                 # UPDATE: handle sret in prologue + Ret
│   └── mod.rs                  # (unchanged)
└── rt_emu/
    └── instance.rs             # UPDATE: sret buffer alloc, arg shift
```

## Conceptual Architecture

### Updated Compile Flow

```
LpsFnSig ──► func_abi_rv32() ──► FuncAbi ──┐
                                           │
                                           ▼
IrFunction ──► lower ──► VInsts ──► regalloc ──► Allocation
                              │      ▲            │
                              │      │            │
                              └──────┴────────────┘
                                   FuncAbi drives
                                   precolors & allocatable

Allocation + FuncAbi ──► emit ──► bytes
    │              │
    │              ▼
    │         FrameLayout::compute()
    │              │
    ▼              ▼
VReg ──► PReg   prologue: mv s1,a0 (sret)
e.g., a0,a1,    Ret: store to buffer or move to regs
      s2-s5
```

### Runtime Flow (Sret)

```
invoke_flat(args) ──► detect sret ──► allocate buffer
    │                                    │
    │                                    ▼
    │                           prepend buffer ptr
    │                           to arg list
    │                                    │
    ▼                                    ▼
call native code ◄────────────────── a0 = buffer ptr
    │
    ▼
callee stores 16 words
    │
    ▼
return ──► read buffer ──► return values
```

## Main Components

### 1. FuncAbi Helpers (Phase 1)

```rust
impl FuncAbi {
    /// Get precolor for a specific vreg (for regalloc)
    pub fn precolor_of(&self, vreg: u32) -> Option<PReg> {
        self.precolors.iter()
            .find(|(v, _)| *v == vreg)
            .map(|(_, p)| *p)
    }

    /// Get sret word count if this is an sret function (for emitter)
    pub fn sret_word_count(&self) -> Option<u32> {
        match &self.return_method {
            ReturnMethod::Sret { word_count, .. } => Some(*word_count),
            _ => None,
        }
    }

    /// Stack frame alignment requirement (for FrameLayout)
    pub fn stack_alignment(&self) -> u32 {
        // RV32 uses 16, could be parameterized by ISA
        16
    }
}
```

### 2. Regalloc Integration (Phase 2)

```rust
// regalloc/greedy.rs
pub fn allocate_with_abi(
    &mut self,
    vinsts: &[VInst],
    vregs: &[VRegInfo],
    abi: &FuncAbi,  // NEW
) -> Allocation {
    // 1. Set up precolored vregs
    for (vreg, preg) in abi.precolors() {
        self.assignments.insert(*vreg, Assignment::Reg(*preg));
    }

    // 2. Only allocate from abi.allocatable() set
    let pool = abi.allocatable();

    // 3. Normal greedy allocation for remaining vregs
    // ...
}
```

### 3. Emitter Integration (Phase 3)

```rust
// isa/rv32/emit.rs
pub fn emit_function(
    &self,
    vinsts: &[VInst],
    alloc: &Allocation,
    abi: &FuncAbi,  // NEW
) -> CodeBlob {
    let frame = FrameLayout::compute(abi, alloc.spill_count(), ...);

    // Prologue
    if abi.is_sret() {
        emit_mv_s1_a0();  // preserve sret pointer
    }
    // ... save callee-saved from frame.callee_save_offsets

    // Body
    for vinst in vinsts {
        match vinst {
            VInst::Ret { vals } => {
                if abi.is_sret() {
                    // Store each val to buffer at s1 + offset
                    for (i, vreg) in vals.iter().enumerate() {
                        let preg = alloc.get(*vreg);
                        emit_sw(preg, S1, i as i32 * 4);
                    }
                } else {
                    // Move to return registers (current behavior)
                    emit_moves_to_a0_a1(vals, alloc);
                }
            }
            // ... other vinsts
        }
    }
}
```

### 4. Runtime Integration (Phase 4)

```rust
// rt_emu/instance.rs
fn invoke_flat(&self, func_name: &str, args: &[Value]) -> Vec<Value> {
    let sig = self.module.meta.signatures.get(func_name).unwrap();
    let abi = func_abi_rv32(sig, entry_param_scalar_count(sig));

    if abi.is_sret() {
        // Allocate sret buffer from arena
        let word_count = abi.sret_word_count().unwrap();
        let buffer = self.memory.alloc(word_count * 4);

        // Shift args: prepend buffer pointer
        let mut shifted_args = vec![Value::Ptr(buffer)];
        shifted_args.extend_from_slice(args);

        // Call with shifted args
        self.call_native(func_name, &shifted_args);

        // Read results from buffer
        read_buffer_to_values(buffer, word_count)
    } else {
        // Direct return - current behavior
        self.call_native(func_name, args)
    }
}
```

## Testing Strategy

1. **Unit tests** in each phase for new methods
2. **Integration tests** using existing filetests:
   - `spill_pressure.glsl` (mat4 return with spilling)
   - `mat4/op-add.glsl` and other mat4 tests
3. **Verify** all 82 existing tests still pass
4. **fw-esp32** build check

## Migration Path

The old `abi.rs` system stays in place during integration. Once abi2 is fully wired and tested, we can:
1. Remove `isa/rv32/abi.rs`
2. Update imports in dependent code
3. Clean up old ABI structures

This will be a separate cleanup plan after integration is validated.
