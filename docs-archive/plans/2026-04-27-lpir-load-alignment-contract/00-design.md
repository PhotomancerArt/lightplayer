# Scope Of Work

Define and enforce a clear LPIR load alignment contract for current scalar
memory loads, then simplify the RV32 Cranelift path to use the same natural
halfword load behavior as `lpvm-native`.

# File Structure

```text
docs/
├── reports/
│   └── 2026-04-27-rv32-load16-issue.md       # UPDATE: reference final chosen contract
└── plans/
    └── 2026-04-27-lpir-load-alignment-contract/
        ├── 00-notes.md
        ├── 00-design.md
        ├── 01-document-lpir-load-alignment.md
        ├── 02-emulator-alignment-guardrails.md
        ├── 03-remove-cranelift-load16-workaround.md
        ├── 04-backend-contract-filetests.md
        └── 05-cleanup-summary-and-validation.md

lp-shader/
├── lpir/
│   └── src/
│       └── lpir_op.rs                         # UPDATE: document load alignment contract
├── lpvm-cranelift/
│   └── src/
│       ├── emit/memory.rs                     # UPDATE: remove Load16U decomposition
│       ├── emit/mod.rs                        # UPDATE: remove context flag
│       └── module_lower.rs                    # UPDATE: stop enabling context flag
├── lpvm-native/
│   └── src/isa/rv32/emit.rs                   # READ/TEST: reference lhu/lh lowering
└── lps-filetests/
    └── filetests/
        └── textures/                          # UPDATE/ADD: aligned texture backend coverage if needed

lp-riscv/
└── lp-riscv-emu/
    └── src/
        └── emu/
            ├── memory.rs                      # READ/TEST: strict alignment behavior
            └── executor/load_store.rs         # TEST: lh/lhu/lw trap/pass coverage
```

# Conceptual Architecture

```text
LPIR memory contract
  Load8*   => any byte address
  Load16*  => 2-byte aligned address required
  Load32   => 4-byte aligned address required
        |
        +--> lpvm-native RV32: direct lh/lhu/lw
        |
        +--> lpvm-cranelift RV32: Cranelift sload16/uload16/load -> lh/lhu/lw
        |
        +--> lpvm-wasm: WASM may execute unaligned loads, but generated LPIR remains aligned by contract
        |
        +--> lpir::interp: may byte-read internally, but tests document LPIR contract
```

# Main Components

## LPIR Contract

`lpir_op.rs` should state the alignment preconditions directly on the load
operations. This is the source of truth for backend authors:

- `Load8U` / `Load8S`: no alignment requirement.
- `Load16U` / `Load16S`: `base + offset` must be 2-byte aligned.
- `Load32` / `Store` of 32-bit values: `base + offset` must be 4-byte aligned
  where applicable.

## Emulator Guardrails

The emulator already rejects misaligned halfword and word loads when
`allow_unaligned_access` is false. Add focused tests so this remains true for
`lh`, `lhu`, and `lw` in the strict default mode used by LPVM filetests.

## Backend Alignment

`lpvm-native` already emits `lhu` / `lh`. `lpvm-cranelift` should do the same
through Cranelift's `uload16` / `sload16` lowering instead of the local
decomposition.

## Texture Coverage

The current texture address calculation remains valid because fixture
allocation and format/channel offsets naturally produce halfword-aligned
addresses. Filetests should continue to pass on `wasm.q32`, `rv32n.q32`, and
`rv32c.q32`.

## Deferred ISA Gating

The emulator should eventually reject instructions outside configured
`rv32imac`; that is broader than this plan. This plan should leave a concise
follow-up note rather than implementing ISA-profile gating.
