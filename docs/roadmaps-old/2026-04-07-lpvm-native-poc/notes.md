# LPVM-Native POC Roadmap Notes

Date: 2026-04-07
Scope: Vertical slice proof-of-concept for custom LPIR→RV32 backend
Target: Compile and run `lps-filetests/filetests/scalar/int/op-add.glsl` through new backend

## Scope of the Effort

Build a minimal functional compiler backend that:

1. Takes LPIR (from `lps-frontend` via Naga)
2. Compiles to RISC-V32 machine code using a simple custom backend
3. Links with existing Q32 builtins
4. Executes in the RISC-V emulator (`lp-riscv-emu`)
5. Passes a single filetest: `op-add.glsl` (simple integer addition)

**NOT in scope for POC:**

- Control flow (if/else, loops)
- Function calls (except builtins)
- Spilling (assume <= 24 live values)
- JIT execution on real hardware (use emulator)
- Complex register allocation (greedy/round-robin is fine)
- Optimizations

## Current State of the Codebase

### Existing infrastructure we can reuse:

- **lps-frontend**: GLSL → LPIR (via Naga) - fully functional
- **lpir crate**: LPIR IR, parser, printer, interpreter - fully functional  
- **lps-filetests**: File-based test harness with `.glsl` → `.q32` execution
- **lp-riscv-emu**: RISC-V 32-bit emulator with syscall support - functional
- **lp-riscv-elf**: ELF loading and linking - can reuse for object files
- **Q32 builtins**: Existing `__lp_*_q32` functions in firmware

### Current Cranelift-based flow:

```
op-add.glsl
    → lps-frontend (Naga parse)
    → LPIR
    → lpvm-cranelift (LPIR → CLIF → RV32 machine code)
    → lp-riscv-elf (link with builtins)
    → lp-riscv-emu (execute)
    → compare output
```

### Filetest structure:

Each `.glsl` file has:

- `# test: q32` header indicating expected result
- Input values and expected outputs in comments
- Harness compiles and executes on three backends: jit.q32, wasm.q32, rv32.q32

## Questions to Answer

### Q1: Output format - JIT buffer or ELF object?

**Context**: We need to produce executable code that can be linked with builtins and run in the emulator. Two options:

**Option A: ELF object file** (like current Cranelift `object_module.rs`)

- Pros: Reuse existing `lp-riscv-elf` linking infrastructure, debuggable, standard format
- Cons: Need to generate relocations, sections, headers; more upfront complexity

**Option B: Raw JIT buffer** (like `jit_module.rs` but for RV32)

- Pros: Simpler code generation (no ELF metadata), direct control
- Cons: Need custom linking logic (patch addresses at runtime), harder to debug

**Suggested**: Start with **ELF** - the linking infrastructure already exists and the complexity is manageable for a POC. We can emit a minimal relocatable ELF with just `.text` and symbol table.

### Q2: Register allocator complexity

**Context**: `op-add.glsl` is a simple shader. We need to decide how sophisticated the allocator needs to be.

**Option A: Greedy single-pass** (minimal)

- Assign registers round-robin as we emit instructions
- Spill to stack when we run out (emergency fallback)
- No interval analysis, no coalescing

**Option B: Linear scan with intervals** (medium)

- Compute live intervals (feasible since no control flow in POC)
- Allocate with simple linear scan
- Better code quality, but more upfront work

**Suggested**: **Greedy for POC**. `op-add.glsl` likely has < 10 live values. We can always upgrade to linear scan in M2.

### Q3: Where does the new backend live?

**Context**: We need a new crate for the backend. Options:

**Option A: New top-level crate `lp-shader/lpvm-native`**

- Clean separation, can be feature-flagged
- Clear that it's an alternative to `lpvm-cranelift`

**Option B: Sub-module within `lpvm-cranelift`**

- Can share some types, but feature flag complexity
- Confusing to have "cranelift" crate without Cranelift

**Option C: Modify `lpvm-cranelift` to support pluggable backends**

- Clean abstraction but more refactoring

**Suggested**: **Option A** - `lp-shader/lpvm-native`. Clean separation, easier to iterate without affecting working code.

### Q4: 64-bit support - include or defer?

**Context**: Earlier discussion mentioned 64-bit passthrough (load/store/copy only) as potentially important to design in upfront.

For `op-add.glsl`:

- The test uses 32-bit integers (`i32`)
- No 64-bit values needed

**Option A: 32-bit only for POC**

- Simpler data model, faster to implement
- Can add 64-bit later with refactoring

**Option B: Include 64-bit in design but stub implementation**

- IR types include `I64`, VInst has variants
- Implementation panics/unimplemented for 64-bit paths
- Prevents architectural refactoring later

**Suggested**: **Option B** - include in design but stub. Add `I64` to types, handle it in lowering (split to I32 pairs), but only implement 32-bit emission paths. This adds ~20% complexity now but prevents painful redesign.

### Q5: Filetest integration - new backend or extend harness?

**Context**: Filetests currently run against 3 backends. We want to add a 4th.

**Option A: Add `native.q32` target to existing filetest harness**

- Integrate into `lps-filetests/src/lib.rs`
- Parallel to `jit.q32`, `wasm.q32`, `rv32.q32`

**Option B: Separate test binary for POC**

- Quick standalone test just for `op-add.glsl`
- Less integration complexity

**Suggested**: **Option A** - extend harness. The POC isn't just about proving it works once; it's about proving it can slot into the existing infrastructure. Adding a 4th backend validates the abstraction.

### Q6: Builtin linking approach

**Context**: `op-add.glsl` in Q32 mode uses `__lp_lpir_fadd_q32` builtin for addition.

**Current Cranelift approach**: Emit relocatable ELF with undefined symbols, link at load time using `lp-riscv-elf`.

**Alternative**: Pre-link at compile time with known builtin addresses (from firmware symbol table).

**Suggested**: Use **existing ELF linking approach**. The `lp-riscv-elf` crate already handles this. Emit an ELF with relocations for builtin calls, link in the emulator.

## Additional Notes from Discussion

- QBE's `rv64/` directory shows ~400 lines for ABI, ~200 for instruction selection, ~300 for emission. Good reference.
- `op-add.glsl` likely compiles to very simple LPIR: just parameter loading, one `fadd` (which calls builtin), return.
- The hard part isn't instruction selection - it's the plumbing (ABI, stack frames, calling convention).
- For POC: NO function calls between user functions (only builtins), NO control flow, NO spilling, NO stack.

## Open Technical Questions

1. Do we emit `.o` format that `lp-riscv-elf` can consume directly, or do we need a custom loader?
2. Should we reuse Cranelift's `lps-builtins` ABI generation or emit our own?
3. How do we get builtin addresses? From `fw-esp32` symbol table at test time?
4. What's the actual LPIR for `op-add.glsl`? Let's inspect it.

## Target Test Case Details

`op-add.glsl` contents (to verify):

```glsl
#version 450 core
float add(float a, float b) {
    return a + b;
}
```

In Q32 mode this becomes:

- Two parameters (Q32 values in i32 registers)
- Call `__lp_lpir_fadd_q32`
- Return result

The test harness will call this with specific values and check output.