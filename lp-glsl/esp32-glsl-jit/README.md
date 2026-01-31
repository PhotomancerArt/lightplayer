# ESP32-C6 GLSL JIT Test

This app tests the **GLSL shader compiler running on real RISC-V hardware** (ESP32-C6).

## What It Does

1. Parses a GLSL fragment shader at runtime
2. Performs semantic analysis and type checking
3. Compiles it to RISC-V32 machine code using Cranelift (in no_std!)
4. Executes the JIT-compiled shader natively on the ESP32-C6
5. Verifies the result

## Test Shader

```glsl
void main() {
    int a = 7;
    int b = 6;
    int result = a * b;
    return result;
}
```

The test compiles this at runtime and executes it to get `42`.

## Hardware Requirements

- **ESP32-C6** dev board (RISC-V32 with 512KB RAM)
- USB cable for programming/debugging

## Building

```bash
cd apps/esp32c6-glsl-jit
cargo build --release
```

## Flashing

```bash
cargo espflash flash --release --monitor
```

Or with specific port:

```bash
cargo espflash flash --release --monitor --port /dev/tty.usbserial-*
```

## Expected Output

```
ESP32-C6 GLSL JIT Test
Testing Cranelift GLSL Compiler on Real RISC-V Hardware!

GLSL Source:
void main() {
    int a = 7;
    int b = 6;
    int result = a * b;
    return result;
}

Step 1: Creating RISC-V32 ISA...
  ✓ ISA created
Step 2: Compiling GLSL to RISC-V machine code...
  ✓ Compilation successful: X bytes
Step 3: Executing JIT-compiled shader...
Calling compiled GLSL shader
Result: 42
Expected: 42
======================================
✅ GLSL JIT TEST SUCCESS ON REAL HARDWARE!
======================================
```

## Memory Configuration

- **Heap**: 128KB (allocated for Cranelift compilation)
- **Stack**: Default ESP-HAL stack
- **Code**: Minimal (varies by shader complexity)

## Technical Details

- **ISA**: RISC-V32IMAC (matches ESP32-C6 core)
- **Compiler**: Cranelift (no_std mode)
- **Frontend**: GLSL parser + semantic analyzer
- **Optimization**: None (for faster compilation)
- **Target**: `riscv32imac-unknown-none-elf`

This proves that:
- ✅ GLSL compilation works in real no_std environments
- ✅ Full compiler pipeline (parse → analyze → codegen) works on embedded
- ✅ Cranelift JIT is practical for shader compilation on RISC-V MCUs
