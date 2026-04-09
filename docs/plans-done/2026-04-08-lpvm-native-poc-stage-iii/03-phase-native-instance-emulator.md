## Phase 3: `NativeEmuModule` — linked image holder

### Scope

Implement `NativeEmuModule` in `rt_emu/module.rs`:
- Hold `IrModule` (for function metadata during calls)
- Hold `Arc<ElfLoadInfo>` (linked code + ram + symbol map)
- Hold `LpsModuleSig` (function signatures)
- Hold `EmuSharedArena` reference (for vmctx allocation)
- Store `NativeCompileOptions`
- Retain object bytes as `_elf: Vec<u8>` (debugging only, unused at runtime)

Implement `LpvmModule::instantiate()`:
- Allocate vmctx slot from arena (16-byte aligned, `GUEST_VMCTX_BYTES`)
- Initialize guest vmctx header (fuel + trap + metadata)
- Return `NativeEmuInstance`

### Code organization

- `rt_emu/module.rs` — module struct + `LpvmModule` impl
- `rt_emu/instance.rs` — instance struct (skeleton for Phase 4)

### Implementation details

```rust
pub struct NativeEmuModule {
    pub(crate) ir: IrModule,
    pub(crate) _elf: Vec<u8>,  // retained for debugging
    pub(crate) meta: LpsModuleSig,
    pub(crate) load: Arc<ElfLoadInfo>,
    pub(crate) arena: EmuSharedArena,
    pub(crate) options: NativeCompileOptions,
}

impl LpvmModule for NativeEmuModule {
    type Instance = NativeEmuInstance;
    type Error = NativeError;

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        // alloc vmctx, init header, return instance
    }
}
```

### Tests

```bash
cargo check -p lpvm-native --features emu
```
