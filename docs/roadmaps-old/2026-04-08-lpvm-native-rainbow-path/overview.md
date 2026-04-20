# LPVM-Native Rainbow Path Roadmap

Date: 2026-04-08

## Motivation / Rationale

The first `lpvm-native` POC proved the approach viable—minimal ELF emission, greedy register allocation, and a working `rv32lp.q32` filetest target. But it only handled `op-add.glsl`: integer add, no control flow, no calls, no vectors.

The real goal is **meaningful comparison with Cranelift** on representative shaders. `rainbow.glsl` is that representative target: vec4 entry, LPFX builtins with out-params, smoothstep, mix, fract, control flow, and a `psrdnoise` call returning values via pointer.

Without completing this path, we cannot validate the core hypothesis: that a custom lightweight backend can reduce compile-time RAM by 10x and binary size by 5x while maintaining acceptable runtime performance.

## Architecture / Design

**Pipeline completion** (same as design doc, extended for full LPIR):

```
GLSL source
    │
    ▼
┌─────────────────────────────────────┐
│ lps-frontend → LPIR                 │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ lpvm-native: lower.rs               │
│ - Full op coverage (rainbow subset) │
│ - Control flow (if/else, loops)     │
│ - Calls (user functions, builtins)  │
│ - Memory (stack slots, out-params)  │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ regalloc/ (greedy → linear scan)    │
│ - Greedy: checkpoint, +spills       │
│ - Linear scan: production quality   │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ isa/rv32/emit.rs                    │
│ - ABI: sret, multi-return, out-args │
│ - Prologue/epilogue with spills     │
│ - Branch relaxation (large offsets) │
└─────────────────────────────────────┘
    │
    ▼
Two paths:
┌─────────────────┐    ┌─────────────────────────┐
│ ELF (host/now)  │ →  │ lp-riscv-elf link + emu │
│ rt_jit (device) │ →  │ JIT buffer + builtin tbl│
└─────────────────┘    └─────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ lp-engine backend selection         │
│ - NativeGraphics impl LpvmEngine    │
│ - Runtime config: cranelift|native  │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ fw-emu / fw-esp32                   │
│ - FPS, memory high-water, power     │
│ - Comparison: same shader, two backends│
└─────────────────────────────────────┘
```

### Key Technical Areas

**ABI Compliance (M1)**: Match Cranelift's RV32 calling convention for LPIR:
- Multi-scalar returns in a0-a1 (vec2), a0-a3 (vec4)
- `sret` pointer in a0 for large returns (>4 scalars)
- Out-parameters (pointer arguments) for builtins like `lpfn_psrdnoise`
- Stack spill slots with proper alignment

**Lowering Coverage (M2)**: Rainbow LPIR requires:
- All arithmetic (integer, Q32 float via builtins)
- Comparison, selection (for `smoothstep`, `mix` control flow)
- Control flow (if/else via branches, no loops in rainbow)
- Function calls (user functions like `paletteHeatmap`, builtins)
- Memory ops (stack slots for spills, pointer args for out-params)

**Register Allocation (M3)**: Linear scan with:
- Live interval construction from VInst sequence
- Spill slot assignment when intervals collide
- Spill code insertion (store on def, load on use)

**JIT Runtime (M4)**: `rt_jit` for on-device compilation:
- Direct machine code buffer emission (no ELF)
- Builtin address resolution at load time
- Compatible with firmware's builtin table

**Integration (M5)**: Full stack wiring:
- `lp-engine` backend trait implementation
- Feature-gated firmware builds (native vs Cranelift)
- FPS and memory measurement in fw-emu/fw-esp32

## Alternatives Considered

| Alternative | Verdict | Rationale |
|-------------|---------|-----------|
| Finish Cranelift optimization instead | Rejected | Already disabled optimizer/verifier, forked regalloc2. Peak RAM ~50KB is structural to regalloc2's algorithm—can't reduce further without replacing allocator entirely. |
| Precompile shaders on host | Rejected | Violates core product requirement (embedded JIT). Also breaks on-device shader editing workflow. |
| Use external minimal compiler (QBE, 8cc) | Rejected | Target C-style irreducible CFG. LPIR's structured control flow enables simpler algorithms—adapting them would be more work than completing native backend. |
| Skip ABI work, use Cranelift's | Rejected | Need matching ABI for library interop (builtin calls, VMContext). Can't piggyback without full Cranelift integration which defeats size goal. |

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Branch relaxation complexity | Medium | Medium | Design includes scratch registers; emit can panic on extreme offset, grow in later phase |
| Spill code gen bugs | Medium | High | Filetests catch quickly; greedy+spill checkpoint reduces blast radius |
| Linear scan quality worse than expected | Low | Medium | Instruction counting gives early warning; can tune heuristics or fall back to graph coloring later |
| `rt_jit` builtin table mismatch | Medium | High | Same contract as existing JIT path; validation in fw-emu first |
| Firmware binary size still too large | Medium | High | LTO, opt-level z, strip; feature-gate backends; measure per-milestone |
| Performance worse than Cranelift | Low | High | FPS measurement is core goal; can tune generated code after correctness |

## Milestones

| Milestone | Goal | Document |
|-----------|------|----------|
| M1 | ABI: sret, multi-return, out-params, stack spills | [m1-abi](m1-abi.md) |
| M2 | Expanded lowering for rainbow LPIR | [m2-lowering](m2-lowering.md) |
| M3 | Linear scan regalloc + spills | [m3-linear-scan](m3-linear-scan.md) |
| M4 | rt_jit JIT buffer path | [m4-rt-jit](m4-rt-jit.md) |
| M5 | lp-engine wiring, fw-emu + fw-esp32 integration | [m5-integration](m5-integration.md) |
| M6 | Cleanup, validation, performance comparison | [m6-validation](m6-validation.md) |

## Scope Estimate

- **Lines of code**: ~4,000-6,000
- **Files**: ~20-25 modifications + new files
- **Time**: 3-4 weeks (per-milestone estimates in detail files)
- **Key complexity**: ABI edge cases, spill code correctness, branch relaxation, firmware integration

## Success Criteria

1. `rainbow.glsl` passes on `rv32lp.q32` target (numeric parity with `jit.q32`)
2. FPS and memory metrics measurable in `fw-emu` (and `fw-esp32`)
3. Compile-time RAM < 10KB peak (vs ~75KB Cranelift)
4. Runtime performance within 2x of Cranelift-generated code
