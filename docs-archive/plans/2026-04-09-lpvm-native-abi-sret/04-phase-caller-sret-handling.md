## Scope of Phase

Implement caller-side sret handling in `rt_emu/instance.rs`: buffer allocation, arg shifting, and reading return values.

## Implementation Details

### Update `NativeEmuInstance::call_q32`

```rust
fn call_q32(&mut self, name: &str, flat: &[i32]) -> Result<Vec<i32>, String> {
    // Find the function signature
    let gfn = self.module.signatures.functions.iter()
        .find(|f| f.name == name)
        .ok_or_else(|| format!("no function named {name}"))?;

    // NEW: Classify return type
    let return_class = abi::ReturnClass::from_lps_types(&gfn.return_type);

    // Prepare VMContext
    let vmctx_vreg = VReg(0);
    let vmctx = self.arena.alloc_vmctx(vmctx_vreg, &self.module.ir);

    // NEW: Handle sret
    let (mut all_args, sret_buffer) = match &return_class {
        abi::ReturnClass::Sret { .. } => {
            // Allocate sret buffer from arena
            let scalar_count = abi::scalar_count_of_lps_type(&gfn.return_type);
            let buffer_size = scalar_count * 4;  // 4 bytes per scalar
            let sret_ptr = self.arena.alloc_sret_buffer(buffer_size as usize);

            // Prepend sret pointer to args (will go in a0)
            let mut args = vec![sret_ptr as i32];
            args.extend_from_slice(flat);
            (args, Some((sret_ptr, scalar_count)))
        }
        abi::ReturnClass::Direct { .. } => {
            // Normal case: args unchanged
            (flat.to_vec(), None)
        }
    };

    // Prepend VMContext (always first after sret, or first if no sret)
    all_args.insert(0, vmctx as i32);

    // NEW: Adjust arg count for sret (vmctx + sret + real_args, or vmctx + real_args)
    // Find the code pointer
    let (sym_base, sym_off) = self.module.load.symbol_base_and_offset(name)
        .ok_or_else(|| format!("function {name} not found in object"))?;
    let func_ptr = (sym_base as usize + sym_off) as *const u8;

    // Call based on arg count
    let arg_count = all_args.len();
    let ret_count = match &return_class {
        abi::ReturnClass::Direct { regs } => regs.len(),
        abi::ReturnClass::Sret { .. } => 0, // Return via buffer, not registers
    };

    // Invoke
    let mut ret_buffer = vec![0i32; ret_count.max(1)]; // At least 1 for simple invoke
    unsafe {
        crate::invoke::invoke_i32_args_returns(
            func_ptr,
            vmctx as *const u8,
            &all_args,
            ret_count,
            &mut ret_buffer,
            sret_buffer.is_some(),  // NEW: pass sret flag
        ).map_err(|e| format!("invoke: {e}"))?;
    }

    // NEW: Read return values from sret buffer if applicable
    let result = match sret_buffer {
        Some((ptr, count)) => {
            // Read scalars from buffer
            let mut result = Vec::with_capacity(count as usize);
            for i in 0..count {
                let offset = (i * 4) as isize;
                let val = unsafe { core::ptr::read_volatile((ptr as *const u8).offset(offset) as *const i32) };
                result.push(val);
            }
            result
        }
        None => {
            // Normal register returns
            ret_buffer
        }
    };

    Ok(result)
}
```

### Add arena method for sret buffer

```rust
// In arena/shared.rs
impl EmuSharedArena {
    /// Allocate a buffer for sret returns.
    /// Size must be a multiple of 4 (alignment).
    pub fn alloc_sret_buffer(&self, size: usize) -> *mut u8 {
        assert!(size % 4 == 0, "sret buffer size must be 4-byte aligned");
        self.alloc_bytes(size)
    }
}
```

## Tests to Write

No new unit tests - filetests will verify the integration:

## Validate

```bash
scripts/filetests.sh --target rv32lp.q32 scalar/spill_pressure.glsl:15
```

This should now pass with mat4 sret return.
