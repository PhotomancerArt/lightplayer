# Stage V1: RV32 Object, Linking, and Emulator (in `lpvm-cranelift`)

## Goal

Add RISC-V object emission, builtins ELF linking, and emulator execution **inside
`lpvm-cranelift`**, validated by **in-crate tests**. This runs **before** filetest
integration (Stage V2) so switching the filetest runner to `lpvm-cranelift` does
not strand the `rv32.q32` / emulator path on the old compiler.

## Suggested plan name

`lpvm-cranelift-stage-v1`

## Scope

**In scope:**

- **Object emission** in `lpvm-cranelift`:
    - `ObjectModule` backend alongside `JITModule`
    - RV32 ISA creation (riscv32 target triple, no host-arch)
    - Same emitter code (target-agnostic CLIF), different module type
    - `object_from_ir(ir, mode) -> Result<Vec<u8>>` (or similar) producing **shader
      object** ELF bytes (unlinked or ready for link — document choice)
- **Builtins linking**:
    - Merge shader object with pre-compiled builtins ELF
    - Port or reuse old crate's `builtins_linker` logic (uses `lp-riscv-elf`)
    - Produce a single linked ELF with resolved `__lp_*` symbols
- **Emulator execution**:
    - Load linked ELF into the RISC-V emulator (`lp-riscv-emu`)
    - Call compiled functions, read results (same patterns as old `cranelift.q32` path)
- **In-crate tests**: hand-built LPIR or small GLSL → LPIR → object → link → run
  in emulator; assert Q32 / scalar results
- **Feature gating**:
    - `std` feature for host JIT path (`cranelift-jit`)
    - `riscv32` / object path for RV32 emission
    - Mirror old crate's feature split where sensible

**Out of scope (Stage V2):**

- **`jit.q32` / `rv32.q32` filetest targets** in `lps-filetests` (runner wiring)
- Embedded readiness (Stage VI-A)
- lp-engine migration / fw-emu (Stage VI-B)
- ESP32 firmware (Stage VI-C)

## Key decisions

- The emitter (`emit/`) is target-agnostic — it produces CLIF. The module backend
  (`JITModule` vs `ObjectModule`) determines the output format.
- Builtins linking follows the same pattern as the old crate: pre-compiled RV32
  builtins ELF + shader object → merged ELF.
- **Order:** V1 completes before V2 so filetests can depend on a single crate for
  both host JIT and RV32 emulator.

## Open questions

- **ObjectModule API**: Raw ELF bytes vs wrapper struct — raw bytes is simplest.
- **Builtins ELF source**: `include_bytes!` from build step vs on-demand build —
  match old crate unless there is a clear win.
- **Shared module trait**: Separate functions for JIT vs object — simpler than a
  trait for Stage V1.

## Deliverables

- `object_from_ir()` (or equivalent) producing linkable RV32 object bytes
- Builtins linking for RV32
- Emulator smoke tests **inside `lpvm-cranelift`**
- Feature layout documented in crate `README` or `lib.rs` docs

## Dependencies

- Stages I–IV — emitter, builtins, Q32, and public `jit` / `JitModule` API as
  needed to share options and metadata
- `lp-riscv-elf` and `lp-riscv-emu` crates (existing)

## Estimated scope

~400 lines (object module, linker port, emulator glue) + debugging RV32 ABI
(struct-return, multi-return, etc.).
