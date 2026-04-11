# Phase 7: Tests and Validation

## Scope

Write comprehensive tests for the M2 emission pipeline: encoding unit tests, objdump round-trip integration test, and end-to-end compile tests using real LPIR from GLSL.

## Code Organization

- Unit tests in source files (existing)
- Integration tests in `tests/emit_tests.rs`
- Optional objdump-based validation

## Tests Implementation

### 1. Unit Tests (in-file)

Already implemented in previous phases:
- `inst.rs`: `test_encode_add`, `test_encode_auipc_jalr`
- `emit.rs`: `test_prologue_epilogue_leaf`, `test_emit_call_records_reloc`
- `greedy.rs`: `test_allocate_within_limit`, `test_allocate_exceeds_limit`
- `engine.rs`: `test_compile_pipeline`, `test_compile_q32_builtin`

### 2. Integration Test: objdump Round-Trip

```rust
// tests/emit_tests.rs
use lpvm_native::{NativeEngine, NativeCompileOptions};
use lpir::IrFunction;
use std::process::Command;

#[test]
fn test_objdump_roundtrip() {
    // Generate simple function
    let engine = NativeEngine::new();
    let func = build_simple_function();
    let module = engine.compile(&func, &NativeCompileOptions::default())
        .expect("compile should succeed");
    
    // Write to temp file
    let path = "/tmp/lpvm_test.o";
    std::fs::write(path, &module.elf).expect("write ELF");
    
    // Try to run objdump (optional - test passes without it)
    if which::which("riscv64-unknown-elf-objdump").is_ok() {
        let output = Command::new("riscv64-unknown-elf-objdump")
            .args(&["-d", "-M", "no-aliases", path])
            .output()
            .expect("objdump execution");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Verify expected instructions appear in disassembly
        assert!(stdout.contains("add"), "Should see add instruction");
        assert!(stdout.contains("jalr"), "Should see jalr for ret");
        
        println!("Disassembly:\n{}", stdout);
    } else {
        println!("riscv64-unknown-elf-objdump not available, skipping visual validation");
    }
}

#[test]
fn test_q32_call_disassembly() {
    let engine = NativeEngine::new();
    let func = build_q32_add_function();
    let module = engine.compile(&func, &NativeCompileOptions::default())
        .expect("compile should succeed");
    
    std::fs::write("/tmp/lpvm_q32.o", &module.elf).expect("write ELF");
    
    if which::which("riscv64-unknown-elf-objdump").is_ok() {
        let output = Command::new("riscv64-unknown-elf-objdump")
            .args(&["-dr", "/tmp/lpvm_q32.o"])  // -r shows relocations
            .output()
            .expect("objdump execution");
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Verify relocation appears
        assert!(stdout.contains("__lpir_fadd_q32"), 
                "Should reference Q32 builtin");
        assert!(stdout.contains("R_RISCV_CALL_PLT"), 
                "Should show CALL_PLT relocation");
    }
}
```

Add to `Cargo.toml` for tests:
```toml
[dev-dependencies]
which = "4.4"  # For detecting objdump availability
```

### 3. End-to-End Test: LPIR from GLSL

```rust
#[test]
fn test_compile_from_glsl_ir() {
    // Use lps-frontend to get real LPIR
    let glsl = r#"
        #version 450
        float main() {
            return 1.0 + 2.0;
        }
    "#;
    
    let naga = lps_frontend::compile(glsl).expect("parse GLSL");
    let (ir, _meta) = lps_frontend::lower(&naga).expect("lower to LPIR");
    
    // Compile first function
    let engine = NativeEngine::new();
    let func = &ir.functions[0];
    let opts = NativeCompileOptions::default();
    
    let module = engine.compile(func, &opts).expect("compile to ELF");
    
    // Verify it's valid ELF with RISC-V architecture
    let obj = object::File::parse(&*module.elf).expect("valid ELF");
    assert_eq!(obj.architecture(), object::Architecture::Riscv32);
    
    // Verify has either code or relocations (for Q32 builtins)
    let has_content = obj.sections().any(|s| {
        s.kind() == object::SectionKind::Text ||
        s.kind() == object::SectionKind::LinkRelocation
    });
    assert!(has_content);
}
```

## Validation Checklist

Run these commands to validate the implementation:

```bash
# 1. Unit tests
cargo test -p lpvm-native --lib

# 2. Integration tests (requires object crate)
cargo test -p lpvm-native --test emit_tests

# 3. Check no_std compatibility
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# 4. Verify ELF output manually (optional)
# cargo test -p lpvm-native --test emit_tests -- --nocapture
# riscv64-unknown-elf-objdump -d /tmp/lpvm_test.o

# 5. Clippy
cargo clippy -p lpvm-native -- -D warnings

# 6. Format
cargo +nightly fmt -p lpvm-native
```

## Expected Test Results

| Test | Expected | Notes |
|------|----------|-------|
| `test_encode_add` | Pass | Hardcoded bit pattern |
| `test_prologue_leaf` | Pass | 12 bytes (3 instructions) |
| `test_prologue_non_leaf` | Pass | 20 bytes (5 instructions) |
| `test_compile_pipeline` | Pass | Full pipeline, valid ELF |
| `test_compile_q32_builtin` | Pass | ELF with R_RISCV_CALL_PLT |
| `objdump_roundtrip` | Pass (skip objdump) | Optional visual check |
| `test_from_glsl_ir` | Pass | End-to-end GLSL → ELF |

## References

- `docs/roadmaps/2026-04-07-lpvm-native-poc/references.md` — Prior art
- `docs/design/native/overview.md` — Architecture overview
