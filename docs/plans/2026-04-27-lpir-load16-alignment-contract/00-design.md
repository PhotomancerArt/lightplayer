# Scope Of Work

This interstitial plan tightens LPIR memory alignment semantics and removes the
temporary `lpvm-cranelift` `Load16U` decomposition added during M3b texture
work.

It does not introduce unaligned load operations and does not implement full
emulator ISA-profile gating. ISA gating should be tracked as follow-up because
it affects every RV32 instruction family, not just narrow loads.

# File Structure

```text
lp-shader/
├── lpir/
│   └── src/
│       ├── lpir_op.rs                 # UPDATE: document natural alignment requirements
│       └── validate.rs                # OPTIONAL: static checks only if local constants make this cheap
├── lpvm-cranelift/
│   └── src/
│       ├── emit/memory.rs             # UPDATE: remove RV32 Load16U decomposition
│       ├── emit/mod.rs                # UPDATE: remove decomposition flag
│       └── module_lower.rs            # UPDATE: stop enabling decomposition for RV32
├── lpvm-native/
│   └── src/
│       └── isa/rv32/emit.rs           # READ/TEST: native Load16U already emits lhu
├── lps-filetests/
│   └── filetests/
│       └── textures/                  # UPDATE/ADD: aligned texelFetch guardrails if needed
└── lpvm-wasm/
    └── src/
        └── emit/ops.rs                # READ/DOC: WASM remains permissive but advertises align=1

lp-riscv/
└── lp-riscv-emu/
    └── src/
        └── emu/
            ├── memory.rs              # TEST: strict read_halfword/read_word alignment behavior
            └── executor/load_store.rs # TEST: lhu/lh/lw traps on misaligned addresses

docs/
├── reports/
│   └── 2026-04-27-rv32-load16-issue.md # UPDATE: record chosen direction
└── plans/
    └── 2026-04-27-lpir-load16-alignment-contract/
        ├── 00-notes.md
        ├── 00-design.md
        ├── 01-document-lpir-alignment-contract.md
        ├── 02-emulator-alignment-guardrails.md
        ├── 03-remove-cranelift-load16-decomposition.md
        ├── 04-backend-contract-filetests.md
        └── 05-cleanup-summary-and-validation.md
```

# Conceptual Architecture Summary

```text
LPIR memory contract
    |
    | Load8*/Store8: byte-addressable
    | Load16*/Store16: address % 2 == 0
    | Load32/Store32: address % 4 == 0
    v
Backends
    |
    | lpvm-native:    Load16U -> lhu
    | lpvm-cranelift: Load16U -> uload16 -> lhu
    | lpvm-wasm:      i32.load16_u align=1, but LPIR contract still requires alignment
    | lpir::interp:   may byte-read internally, but tests should not rely on odd addresses
    v
Validation
    |
    | emulator strict mode traps misaligned lh/lhu/lw
    | texture filetests exercise aligned Load16U on rv32n and rv32c
    v
Device confidence
```

# Main Components

## LPIR Contract

The LPIR op documentation should say alignment is a semantic requirement for
narrow and word loads/stores. This keeps the embedded RV32 path simple and fast.

## Emulator Guardrails

The emulator already has strict alignment checks in `Memory::read_halfword` and
`Memory::read_word`. This plan adds or strengthens tests so future changes do
not make strict mode accidentally permissive.

## Cranelift RV32 Lowering

`lpvm-cranelift` should stop decomposing `Load16U`. The normal Cranelift path
should emit a legal RV32 halfword load for aligned `Load16U`.

## Backend/Filetest Coverage

The current texture path should remain green on `rv32n.q32`, `rv32c.q32`, and
`wasm.q32`. Tests should demonstrate aligned texture channel reads rather than
depending on unaligned behavior.

## Follow-Up Tracking

The report should clearly say that explicit `rv32imac` ISA-profile gating in
the emulator remains follow-up work. That prevents this smaller plan from
absorbing a broader instruction-set validation project.
