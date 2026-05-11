# Phase 4: Add CraneliftInstance (LpvmInstance implementation)

## Goal

Create `CraneliftInstance` that implements `LpvmInstance`. This holds the
VMContext, memory, and provides the callable interface.

## Design

```rust
pub struct CraneliftInstance {
    // Memory buffer for VMContext (fuel, globals, uniforms)
    memory: Vec<u8>,
    // Pointer passed to functions (points to memory[0])
    vmctx_ptr: *mut u8,
    // Reference to module for function lookups
    module: Arc<CraneliftModule>,
}

impl LpvmInstance for CraneliftInstance {
    type Error = CallError;
    
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error> {
        // 1. Look up function pointer and signature
        // 2. Marshal args to i32 buffer
        // 3. Call via invoke_i32_args_returns
        // 4. Marshal return value
    }
    
    fn set_fuel(&mut self, fuel: u64) {
        // Write fuel to VMContext at offset 0
        let fuel_ptr = self.vmctx_ptr as *mut u64;
        unsafe { *fuel_ptr = fuel; }
    }
    
    fn get_fuel(&self) -> u64 {
        let fuel_ptr = self.vmctx_ptr as *const u64;
        unsafe { *fuel_ptr }
    }
}
```

## VMContext Layout

```
Offset 0:   fuel (u64) - execution fuel counter
Offset 8:   globals pointer (i32) - stub for now
Offset 12: uniforms pointer (i32) - stub for now
Offset 16: scratch area...
```

## Implementation Notes

- Memory is `Vec<u8>` owned by instance
- Use existing `invoke.rs` for calling conventions
- Value marshaling: reuse `GlslQ32` or create new `LpsValue` marshaling
- Fuel: optional, stub if not needed immediately

## Files to Create/Modify

- `lp-shader/lpvm-cranelift/src/instance.rs` (NEW)
- `lp-shader/lpvm-cranelift/src/lib.rs` (add module)

## Tests

```rust
#[test]
fn test_instance_call_i32_add() {
    let module = compile_i32_add_module();
    let mut inst = module.instantiate().unwrap();
    
    let result = inst.call("add", &[LpsValue::I32(5), LpsValue::I32(3)]).unwrap();
    assert_eq!(result, LpsValue::I32(8));
}

#[test]
fn test_instance_fuel() {
    let module = compile_simple_module();
    let mut inst = module.instantiate().unwrap();
    
    inst.set_fuel(1000);
    assert_eq!(inst.get_fuel(), 1000);
}

#[test]
fn test_two_instances_independent() {
    let module = compile_counter_module();
    let mut inst1 = module.instantiate().unwrap();
    let mut inst2 = module.instantiate().unwrap();
    
    // Each instance has separate memory/state
    inst1.call("inc", &[]).unwrap();
    inst1.call("inc", &[]).unwrap();
    inst2.call("inc", &[]).unwrap();
    
    assert_eq!(inst1.call("get", &[]).unwrap(), LpsValue::I32(2));
    assert_eq!(inst2.call("get", &[]).unwrap(), LpsValue::I32(1));
}
```

## Done When

- `CraneliftInstance` implements `LpvmInstance`
- Can call functions via trait interface
- VMContext (fuel) works
- Multiple instances are independent
- Unit tests pass
