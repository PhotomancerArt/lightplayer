# Milestone 5: lp-engine Wiring and Firmware Integration

**Goal**: Wire native backend into `lp-engine` for backend selection, integrate into `fw-emu` and `fw-esp32` for real measurements.

## Suggested Plan

`lpvm-native-integration-m5`

## Scope

### In Scope

- **lp-engine backend**: `NativeGraphics` implementing `LpvmEngine` trait
- **Runtime selection**: Project/runtime configuration to choose cranelift vs native
- **fw-emu feature**: `native-graphics` feature flag for emulator firmware
- **fw-esp32 feature**: `native-graphics` feature flag for ESP32 firmware
- **FPS measurement**: Shader execution timing in firmware scene renderer
- **Memory measurement**: Peak RAM usage (compile + runtime) via tracing

### Out of Scope

- Dynamic backend switching (compile-time selection only)
- Hybrid backends (not in this roadmap)
- Power measurement (may need separate hardware)

## Key Decisions

1. **Feature flag**: `native-graphics` on `lp-engine`, `fw-emu`, `fw-esp32`
2. **Default**: Cranelift remains default; native is opt-in for testing
3. **Metrics**: Same measurement infrastructure for both backends (comparable numbers)
4. **Ordering**: `fw-emu` lands first (easier iteration), then `fw-esp32`

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| `NativeGraphics` | `lp-engine/src/graphics/native.rs` | LpvmEngine implementation |
| Engine selection | `lp-engine/src/runtime/` | Runtime config for backend choice |
| fw-emu feature | `fw-emu/Cargo.toml`, `src/` | `native-graphics` feature gating |
| fw-esp32 feature | `fw-esp32/Cargo.toml`, `src/` | `native-graphics` feature gating |
| FPS counter | `fw-tests/` | Scene render timing for both backends |
| Memory tracer | `lpvm-native/` | Compile-time RAM tracking (peak allocator usage) |
| Comparison report | `docs/reports/` | FPS and memory comparison: Cranelift vs Native |

## Dependencies

- M4: JIT buffer compilation working
- M3: Linear scan producing good code
- M2: Rainbow passes filetests

## Estimated Scope

- **Lines**: ~600-1000
- **Files**: 6-10 modified (engine, firmware configs, tests)
- **Time**: 3-5 days

## Acceptance Criteria

1. `cargo test -p fw-tests --features native-graphics` passes
2. `cargo check -p fw-esp32 --features native-graphics,esp32c6` passes
3. Rainbow shader runs in `fw-emu` with both backends
4. FPS and memory numbers extractable from test runs
5. Numeric correctness: native output matches cranelift (within tolerance)
