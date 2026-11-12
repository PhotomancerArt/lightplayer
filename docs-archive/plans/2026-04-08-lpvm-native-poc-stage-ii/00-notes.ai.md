# Plan notes: lpvm-native POC — stage ii (M2 RV32 Emission)

Roadmap: `docs/roadmaps/2026-04-07-lpvm-native-poc/m2-rv32-emission.md`  
M1 Plan: `docs/plans/2026-04-08-lpvm-native-poc-stage-i/` (completed)

## Scope of work

Implement RV32 instruction encoding and emission pipeline:
- R/I/S/B/U/J instruction encoding functions
- `isa/rv32/inst.rs` with encode helpers (no state, pure functions)
- `isa/rv32/emit.rs` with `EmitContext` and `emit_vinst()` → `Vec<u8>`
- Shader ABI: frame layout, prologue/epilogue emission
- Builtin call emission with relocation records (symbol name + offset)
- Stack slot management for emergency spills
- Integration: `NativeEngine::compile()` fills in lowering → regalloc → emission

**In scope for this POC stage:** Simple shaders without control flow. `op-add.glsl` target.

**Explicitly out of scope:**
- Control flow (branches, jumps, loops) — deferred to later milestone
- 64-bit operations (I64Stub) — stub/panic
- ELF container — raw bytes + metadata
- Actual linking — just relocation records for later resolution

## Current codebase state

- **M1 complete:** `lpvm-native` crate exists with `VInst`, lowering, `GreedyAlloc`, RV32 ABI definitions in `isa/rv32/abi.rs`
- **Instruction encoding:** Not implemented (empty `isa/rv32/emit.rs` stub)
- **Emission context:** Not implemented
- **Frame layout:** ABI module has `FrameLayout` type but no emission logic
- **Relocations:** Not defined yet

## Questions

### Q1 — Instruction encoder source and validation strategy?

**Context:** We need instruction encoding functions for R/I/S/B/U/J formats. Options:

- **A:** Write from scratch using RISC-V spec bit diagrams
- **B:** Adapt from our Cranelift fork — we know it works
- **C:** Use a Rust RISC-V assembler crate

**Assessment:** The Cranelift fork at `/Users/yona/dev/photomancer/lp-cranelift/cranelift/codegen/src/isa/riscv32/inst/encode.rs` has exactly what we need: pure functions like `encode_r_type_bits`, `encode_i_type_bits`, `encode_s_type` with clear bit-layout comments. No ISLE dependencies for the core encoders.

**Answer:** **Adapt from Cranelift fork** — extract the R/I/S/B/U/J encoding functions, strip out compressed/vector/float variants we don't need. The register-to-number mapping (`reg_to_gpr_num`) is also straightforward to borrow.

**Validation strategy:**
- **Unit tests:** Hardcoded expected values for common instructions (e.g., `add x1, x2, x3` = `0x003100b3`)
- **CI-optional:** `riscv64-unknown-elf-objdump` round-trip if available (not a hard dependency)
- **Reference:** RISC-V spec chapter 2 instruction listing for bit patterns

---

### Q2 — Frame layout and prologue complexity for POC?

**Context:** The roadmap describes a full frame layout: `[saved ra] [saved s0] [spill slots...] [padding to 16B]`. For `op-add.glsl` (simple, no calls or spills in the happy path), we may not need any frame. But we need to handle:
- Non-leaf functions (calls builtins like `__lp_lpir_fadd_q32`) — need to save/restore `ra`
- Emergency spills if greedy allocator runs out of registers

**Simplification options:**
- **A (minimal):** Only support leaf functions in M2. `op-add.glsl` in Q32 mode calls a builtin → non-leaf. Requires `ra` save/restore.
- **B:** Support non-leaf with fixed frame size. Always save `ra`, optionally `s0` if we use it as FP (we don't need FP without complex spills).
- **C:** Full general frame layout with dynamic sizing based on spills and saved registers.

**Suggested:** **B** — always emit `ra` save/restore for non-leaf, use fixed frame size = `16` bytes aligned (enough for `ra` + 3 spill slots or padding). Emergency spill logic emits explicit `sw`/`lw` with stack offsets. No `s0` frame pointer for M2 (offsets are small, direct `sp`-relative).

**Answer:** **B** — Non-leaf support with 16-byte fixed frame. `op-add.glsl` in Q32 mode calls builtins (`__lpir_fadd_q32`), so `ra` save/restore is required. Sp-relative addressing for slots.

---

### Q3 — Relocation format for builtin calls?

**Context:** We emit `jal` or `auipc+jalr` for builtin calls, but the target address isn't known at compile time. Need to support both JIT (runtime linking) and ELF (static linking) modes, like Cranelift does.

**Prior art:**
- **Cranelift JIT** (`jit/src/compiled_blob.rs`): Stores `Vec<ModuleReloc>` with `{ kind, offset, name, addend }`. At load time, `perform_relocations(get_address_fn)` resolves symbol names to runtime addresses and patches instruction bytes (e.g., `RiscvCallPlt` patches auipc+jalr pair).
- **Cranelift ELF** (`object/src/backend.rs`): Uses `object` crate to create proper ELF relocations (R_RISCV_CALL_PLT, etc.) which the linker processes.
- **QBE**: Outputs assembly text, uses assembler relocations (`call __lpir_fadd_q32` with `%tprel_hi`/`%tprel_lo` modifiers for address loading).
- **RISC-V psABI** (`riscv-elf-psabi-doc`): Defines `R_RISCV_CALL_PLT` for auipc+jalr pairs, `R_RISCV_JAL` for direct jumps. Linker relaxation can convert auipc+jalr to jal if within ±1MB.

**Options:**
- **A:** Internal `NativeReloc` enum (offset + symbol + kind), consumed differently by JIT vs ELF backends. JIT patches directly; ELF converts to `object::Relocation`.
- **B:** Always emit ELF-format relocations, convert for JIT use. More complex, unnecessary indirection.
- **C:** Use `jal` only (±1MB range), emit placeholder, patch at load time. Simpler but limited range.

**Suggested:** **A** — Cranelift's approach is proven. Define `NativeReloc { offset, symbol, kind }` in emission output. ELF backend converts to `object::Relocation` (R_RISCV_CALL_PLT). For M2 POC, **ELF is the primary output** — the emulator consumes ELF for validation. JIT deferred to later milestone. Instruction sequence: `auipc ra, 0; jalr ra, ra, 0` with R_RISCV_CALL_PLT reloc covering both instructions.

**Answer:** _pending_

---

### Q4 — Greedy allocator spill handling in emission?

**Context:** `GreedyAlloc` is round-robin. If we have >24 live values, it would need to spill. Currently it just... what does it do? Looking at current implementation:

```rust
// In greedy.rs allocate()
let phys = ALLOCA_REGS[next % ALLOCA_REGS.len()];
```

This reuses registers if we wrap around! That's wrong for values that are still live. We need to either:
- Fix greedy to track liveness (hard, defeats "greedy" simplicity)
- Add spill logic: when exhausted, spill to stack, reload when needed
- Limit M2 to functions with ≤24 live values, panic/error if exceeded

**Suggested:** For M2 POC targeting `op-add.glsl`, **limit to ≤24 live values, error if exceeded**. `op-add.glsl` is tiny. Add spill infrastructure in M2 only if trivial; otherwise defer to linear scan milestone.

**Answer:** **Limit to 24 live values, error if exceeded** — `op-add.glsl` has ~5 live values. Spill/reload infrastructure deferred to linear scan milestone (M3+). Greedy allocator panic if live set exceeds available registers.

---

### Q5 — Test strategy for end-to-end emission?

**Context:** We want to validate that `op-add.glsl` compiles through the pipeline and produces bytes we can hand-check or run.

**Approach ideas:**
- **Unit test:** Mock `IrFunction` with `iadd` + `return`, run through lowering → regalloc → emission, assert on bytes or disassembly string.
- **Filetest integration:** Add `rv32lp.q32` backend to `lps-filetests`, but that requires working execution in emulator (M3 scope).
- **Manual inspection:** Emit to file, use `riscv64-unknown-elf-objdump -d` to verify.

**Suggested:** Unit tests with hardcoded expected byte sequences for simple functions. One integration test that writes to `/tmp/` and shell-outs to `objdump` if available (optional, CI-gated).

**Answer:** **Hardcoded unit tests + optional objdump round-trip** — Core instruction encodings verified against known bit patterns (e.g., `add x1, x2, x3` = `0x003100b3`). Integration test dumps to `/tmp/` and uses `riscv64-unknown-elf-objdump -d` for visual validation if available, but not required for CI pass.

## References

- **QBE rv64/emit.c:** https://github.com/michg/qbe_riscv32_64/blob/master/rv64/emit.c — pure functions, switch on opcode, emit bytes to buffer
- **RISC-V spec:** https://riscv.org/technical/specifications/ — instruction formats
- **Current M1:** `lp-shader/lpvm-native/src/isa/rv32/abi.rs` — register roles, `FrameLayout`
