## Scope of Phase

Add ergonomic helper methods to `FuncAbi` that will be used repeatedly during integration.

## Code Organization Reminders

- Place new methods after the `new_raw` constructor and before accessor methods
- Keep methods simple and focused
- Add unit tests at the bottom of the test module
- Test both direct and sret cases

## Implementation Details

### Changes to `abi2/func_abi.rs`

Add three helper methods to `impl FuncAbi`:

```rust
impl FuncAbi {
    /// Raw constructor for ISA-specific modules (existing).
    pub(crate) fn new_raw(...) -> Self { ... }

    /// Get precolor for a specific vreg (O(1) lookup for regalloc).
    ///
    /// Returns the physical register this vreg is forced to use due to ABI,
    /// or None if the vreg is not precolored (free for allocation).
    pub fn precolor_of(&self, vreg: u32) -> Option<PReg> {
        self.precolors
            .iter()
            .find(|(v, _)| *v == vreg)
            .map(|(_, p)| *p)
    }

    /// Get sret word count if this is an sret function.
    ///
    /// Returns None for direct returns, Some(word_count) for sret.
    /// The word count tells the emitter how many stores to emit.
    pub fn sret_word_count(&self) -> Option<u32> {
        match &self.return_method {
            ReturnMethod::Sret { word_count, .. } => Some(*word_count),
            _ => None,
        }
    }

    /// Stack frame alignment requirement.
    ///
    /// Currently hardcoded to 16 bytes for RV32. Can be parameterized
    /// by ISA when additional targets are added.
    pub fn stack_alignment(&self) -> u32 {
        16
    }

    // ... existing accessor methods ...
}
```

### Tests

Add these tests to the `#[cfg(test)] mod tests` at the bottom:

```rust
#[test]
fn precolor_of_returns_correct_reg() {
    // Direct return: vmctx in a0
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Float,
        parameters: vec![],
    };
    let abi = rv32::func_abi_rv32(&sig, 1);
    
    // Vreg 0 (vmctx) should be forced to a0
    assert_eq!(abi.precolor_of(0), Some(rv32::A0));
    // Non-precolored vreg returns None
    assert_eq!(abi.precolor_of(999), None);
}

#[test]
fn precolor_of_shifts_for_sret() {
    // Sret: vmctx moves to a1
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Vec4,
        parameters: vec![],
    };
    let abi = rv32::func_abi_rv32(&sig, 1);
    
    // For sret, vmctx is in a1 (a0 holds sret pointer)
    assert_eq!(abi.precolor_of(0), Some(rv32::A1));
}

#[test]
fn sret_word_count_for_mat4() {
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Mat4,
        parameters: vec![],
    };
    let abi = rv32::func_abi_rv32(&sig, 1);
    
    // mat4 = 16 scalars
    assert_eq!(abi.sret_word_count(), Some(16));
}

#[test]
fn sret_word_count_for_vec3() {
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Vec3,
        parameters: vec![],
    };
    let abi = rv32::func_abi_rv32(&sig, 1);
    
    // vec3 = 3 scalars, uses sret
    assert_eq!(abi.sret_word_count(), Some(3));
}

#[test]
fn no_sret_word_count_for_direct() {
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Float,
        parameters: vec![],
    };
    let abi = rv32::func_abi_rv32(&sig, 1);
    
    // Float uses direct return
    assert_eq!(abi.sret_word_count(), None);
}

#[test]
fn stack_alignment_is_16() {
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Float,
        parameters: vec![],
    };
    let abi = rv32::func_abi_rv32(&sig, 1);
    
    assert_eq!(abi.stack_alignment(), 16);
}
```

## Validate

```bash
cargo test -p lpvm-native -- abi::func_abi::tests
```

Should show 10 tests passing (5 existing + 5 new).

```bash
cargo check -p lpvm-native
```

Should have no warnings.

```bash
cargo +nightly fmt -p lpvm-native
```
