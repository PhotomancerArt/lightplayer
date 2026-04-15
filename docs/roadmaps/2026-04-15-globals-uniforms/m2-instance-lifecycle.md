# Milestone 2: VMContext Allocation + Instance Lifecycle

## Goal

Instances allocate VMContext buffers large enough for uniforms, globals, and
the globals snapshot. The instance API supports setting uniforms, running
`__shader_init`, snapshotting globals, and resetting globals before each call.
Filetests with constant-initialized globals pass.

## Suggested Plan Name

`globals-uniforms-m2`

## Scope

### In scope

- **VMContext allocation**: When instantiating a module, compute
  `total = VMCTX_HEADER_SIZE + uniforms_size + 2 * globals_size` from the
  module metadata (from M1). Allocate a buffer of that size. Initialize the
  header as today; zero the rest.

- **Offset constants on the instance**: Store `uniforms_offset`,
  `globals_offset`, `snapshot_offset`, `globals_size` on the instance (or
  derive from module metadata) so lifecycle methods know where to read/write.

- **`set_uniform` API**: Write typed values to the uniforms region. Use
  `LpvmDataQ32` write logic (or raw byte writes at known offsets). Expose
  on both `LpvmInstance` trait (generic) and concrete instance types.

- **`init_globals`**: Call `__shader_init` via the existing call mechanism,
  then memcpy `globals_region → snapshot_region`.

- **`reset_globals`**: memcpy `snapshot_region → globals_region`. No-op when
  `globals_size == 0`.

- **Per-call reset**: The generic `call`/`call_q32` path calls `reset_globals`
  before each invocation.

- **All backends**: Wire this up for `NativeJitInstance`, `CraneliftInstance`,
  `EmuInstance`. The WASM backend can be stubbed/deferred if needed.

- **Filetest runner integration**: The runner calls `init_globals()` once per
  module instantiation (or per `// run:` line — see M3 for full filetest
  design). Constant-initialized global filetests should now pass.

- **Un-gate basic filetests**: Remove `@unimplemented` from global filetests
  that use only constant initializers and private globals (no uniforms needed).

### Out of scope

- `// set_uniform:` filetest directive (M3).
- Engine render loop integration (M3).
- Uniform-dependent global filetests (M3).

## Key Decisions

- The instance owns the full VMContext buffer (including snapshot). This is
  already the case for `NativeJitInstance` (`vmctx_guest` points to the buffer)
  and `CraneliftInstance` (`vmctx_buf: Vec<u8>`).

- `reset_globals` is a host-side memcpy (not a JIT'd call). On RV32 native JIT,
  the buffer is in the process's address space (mapped 1:1 for the JIT), so
  host memcpy works directly.

- For the generic `LpvmInstance` path, reset happens before every `call`. This
  ensures each filetest `// run:` line gets fresh globals. The caller can also
  call `init_globals` explicitly to re-run initializers (e.g. after changing
  uniforms).

## Deliverables

- Updated `lpvm/src/vmcontext.rs` — remove `unimplemented!` stubs, add offset
  helpers or document that offsets come from module metadata.
- Updated instance types (`NativeJitInstance`, `CraneliftInstance`,
  `EmuInstance`) — larger VMContext allocation, lifecycle methods.
- Updated `LpvmInstance` trait or a new `LpvmInstanceExt` trait with
  `set_uniform`, `init_globals`, `reset_globals`.
- Updated filetest runner — call `init_globals` before tests.
- Un-gated constant-initialized global filetests passing on at least rv32n
  and rv32c backends.

## Dependencies

- M1 (frontend lowering): module metadata with layout info, `__shader_init`
  function in LPIR, correct Load/Store offsets in user functions.

## Estimated Scope

~300-500 lines across `lpvm`, `lpvm-native`, `lpvm-cranelift`, `lpvm-emu`.
~50-100 lines filetest runner changes.

## Agent Execution Notes

This milestone is suitable for a single agent session. The agent should:

1. Read M1's output: the `LpsModuleSig` globals metadata, understand the
   layout (uniforms_size, globals_size, offsets).
2. Read `lpvm/src/vmcontext.rs` for current allocation logic.
3. Read each instance implementation to understand current VMContext buffer
   allocation.
4. Implement VMContext allocation changes first, then lifecycle methods, then
   filetest runner integration.
5. Run global filetests incrementally: start with `global/declare-simple.glsl`,
   then `global/access-read.glsl`, then broader.
6. Verify: `cargo test -p lps-filetests` with global tests un-gated.
7. Verify: `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf
   --profile release-esp32 --features esp32c6,server` still builds.
