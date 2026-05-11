## Scope of Phase

Update the emulator runtime to handle sret buffer allocation, argument shifting, and result readback.

## Code Organization Reminders

- Update `invoke_flat` to branch on `abi.is_sret()`
- Add `invoke_sret` helper method
- Add `shift_args_for_sret` helper at bottom
- Add `read_sret_buffer` helper at bottom
- Keep `invoke_direct` as existing behavior

## Implementation Details

### Changes to `rt_emu/instance.rs`

#### 1. Update imports

```rust
use crate::abi2::classify::entry_param_scalar_count;
use crate::isa::rv32::abi2::func_abi_rv32;
use lps_shared::{LpsFnSig, FnParam, LpsType};
```

#### 2. Update `invoke_flat` signature and implementation

```rust
pub fn invoke_flat(&self, func_name: &str, args: &[Value]) -> Result<Vec<Value>, String> {
    // Look up function signature
    let sig = self.module.meta.signatures.get(func_name)
        .ok_or_else(|| format!("Function '{}' not found in module", func_name))?;
    
    // Build FuncAbi
    let param_slots = entry_param_scalar_count(sig);
    let abi = func_abi_rv32(sig, param_slots);
    
    // Branch based on return method
    if abi.is_sret() {
        self.invoke_sret(func_name, sig, &abi, args)
    } else {
        self.invoke_direct(func_name, args)
    }
}
```

#### 3. Add `invoke_sret` method

```rust
fn invoke_sret(
    &self,
    func_name: &str,
    sig: &LpsFnSig,
    abi: &FuncAbi,
    args: &[Value],
) -> Result<Vec<Value>, String> {
    // 1. Get sret word count
    let word_count = abi.sret_word_count()
        .ok_or("sret function missing word_count")?;
    
    // 2. Allocate sret buffer from arena
    let buffer_size = (word_count * 4) as usize;
    let buffer_ptr = self.memory.alloc(buffer_size)
        .ok_or("Failed to allocate sret buffer from arena")?;
    
    // 3. Shift arguments: prepend sret pointer
    let shifted_args = shift_args_for_sret(args, Value::Ptr(buffer_ptr));
    
    // 4. Call the native function
    // This invokes the JIT code which expects:
    //   a0 = vmctx (unchanged)
    //   a1 = sret buffer pointer
    //   a2-a7 = actual arguments
    self.invoke_native(func_name, &shifted_args)?;
    
    // 5. Read results from buffer
    let results = read_sret_buffer(&self.memory, buffer_ptr, word_count)?;
    
    // 6. Optionally free buffer (arena reset handles this)
    // self.memory.free(buffer_ptr, buffer_size);
    
    Ok(results)
}
```

#### 4. Keep `invoke_direct` (existing behavior)

```rust
fn invoke_direct(&self, func_name: &str, args: &[Value]) -> Result<Vec<Value>, String> {
    // Existing implementation - unchanged
    self.invoke_native(func_name, args)
}
```

#### 5. Add helper: shift_args_for_sret

```rust
/// Prepend sret pointer to argument list.
///
/// Arguments arrive as: [vmctx, arg0, arg1, arg2, ...]
/// Must become:        [vmctx, sret_ptr, arg0, arg1, arg2, ...]
///
/// ABI expects: a0 = vmctx, a1 = sret_ptr, a2 = arg0, ...
fn shift_args_for_sret(args: &[Value], sret_ptr: Value) -> Vec<Value> {
    if args.is_empty() {
        // No args, just vmctx not present? This shouldn't happen
        return vec![sret_ptr];
    }
    
    let mut shifted = Vec::with_capacity(args.len() + 1);
    
    // First arg (vmctx) stays first -> a0
    shifted.push(args[0].clone());
    
    // Insert sret pointer -> a1
    shifted.push(sret_ptr);
    
    // Remaining args follow -> a2-a7
    shifted.extend_from_slice(&args[1..]);
    
    shifted
}
```

#### 6. Add helper: read_sret_buffer

```rust
/// Read word_count 32-bit values from sret buffer.
fn read_sret_buffer(
    memory: &dyn LpvmMemory,
    buffer_ptr: u32,
    word_count: u32,
) -> Result<Vec<Value>, String> {
    let mut results = Vec::with_capacity(word_count as usize);
    
    for i in 0..word_count {
        let addr = buffer_ptr.wrapping_add(i * 4);
        let word = memory.read_u32(addr)
            .ok_or_else(|| format!("Failed to read sret buffer at offset {}", i * 4))?;
        results.push(Value::U32(word));
    }
    
    Ok(results)
}
```

### Key Implementation Notes

**Argument Layout:**
- `args[0]` is always vmctx (present even for functions with no explicit params)
- For sret: vmctx stays in a0, sret buffer pointer goes in a1
- This matches what `classify_params(sig, is_sret=true)` produces

**Buffer Management:**
- Allocated from arena using existing `LpvmMemory::alloc()`
- Size is `word_count * 4` bytes (32-bit words)
- Caller reads results before freeing/resetting arena

**Value Types:**
- Return flat `Vec<Value::U32>` for now
- Caller (shader dispatch) can reconstruct vec4/mat4 from scalars
- Future: add type-aware reconstruction

### Testing Strategy

1. **Unit tests** for helpers:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn shift_args_prepends_sret_ptr() {
        let args = vec![
            Value::Ptr(0x1000), // vmctx
            Value::U32(42),     // arg0
            Value::U32(100),    // arg1
        ];
        let sret = Value::Ptr(0x2000);
        
        let shifted = shift_args_for_sret(&args, sret);
        
        assert_eq!(shifted.len(), 4);
        assert_eq!(shifted[0], Value::Ptr(0x1000)); // vmctx unchanged
        assert_eq!(shifted[1], Value::Ptr(0x2000)); // sret inserted
        assert_eq!(shifted[2], Value::U32(42));     // arg0
        assert_eq!(shifted[3], Value::U32(100));    // arg1
    }
    
    #[test]
    fn read_sret_buffer_reads_all_words() {
        // Setup mock memory with buffer contents
        // Verify all words read correctly
    }
}
```

2. **Integration tests** using existing filetests:

Once integrated, run:
- `spill_pressure.glsl` (mat4 return with spilling)
- `mat4/op-add.glsl` and other mat4 tests

## Validate

```bash
# Build
cargo check -p lpvm-native

# Tests
cargo test -p lpvm-native -- rt_emu::

# All tests
cargo test -p lpvm-native

# Format
cargo +nightly fmt -p lpvm-native
```
