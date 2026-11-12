# Phase 6: Update Firmware Crates

## Scope

Update `fw-esp32` and `fw-emu` to create `CraneliftGraphics` and inject it into `LpServer`.

## Files

```
lp-fw/fw-esp32/src/main.rs         # UPDATE: create CraneliftGraphics, pass to LpServer
lp-fw/fw-emu/src/main.rs             # UPDATE: create CraneliftGraphics, pass to LpServer
```

## fw-esp32 Changes

```rust
use alloc::rc::Rc;
use lp_engine::gfx::cranelift::CraneliftGraphics;
use lp_server::LpServer;

#[entry]
fn main() -> ! {
    // ... existing initialization (logger, alloc, peripherals, wifi) ...

    let graphics = Rc::new(CraneliftGraphics::new()); // Create graphics backend

    let mut server = LpServer::new(
        || Box::new(SpiFlashFs::new(/* ... */)),
        graphics,                                           // Pass graphics
    );

    server.start_network(/* ... */);
    server.run_loop();
}
```

## fw-emu Changes

```rust
use alloc::rc::Rc;
use lp_engine::gfx::cranelift::CraneliftGraphics;
use lp_server::LpServer;

fn main() {
    // ... existing initialization (logger, alloc, file system) ...

    let graphics = Rc::new(CraneliftGraphics::new()); // Create graphics backend

    let mut server = LpServer::new(
        || Box::new(HostFileSystem::new("./fixtures")),
        graphics,                                           // Pass graphics
    );

    server.start_network(/* ... */);
    server.run_loop();
}
```

## Type Alias (Optional)

For cleaner firmware code, we might add a type alias:

```rust
// In fw-esp32 or shared helper
#[cfg(feature = "cranelift")]
type DefaultGraphics = lp_engine::gfx::cranelift::CraneliftGraphics;
```

But since we're not doing multi-backend yet, just use direct type.

## Validation

After both firmware files updated:

```bash
# Emulator build (full test with graphics)cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu

# ESP32 compile check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Firmware emulator check
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

All should pass now that graphics is wired through.
