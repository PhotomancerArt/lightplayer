# P6 — `lpvm-emu`: host marshalling for aggregate ABI

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md` (Q4 records the decision).
Depends on: P1 (LPIR sret marker), P3 (frontend), P4 (Cranelift —
`lpvm-emu` reuses Cranelift codegen for the guest, so its instructions
need to be sret-correct first).
Parallel with: P7 (`lpvm-wasm`).

## Scope of phase

Update the host-side call paths in `lpvm-emu` so that aggregate args /
returns marshal through `LpvmDataQ32` and the emulator's shared arena.
Codegen for the guest (RV32) comes free from `lpvm-cranelift` (P4); this
phase only touches the host-side `EmuInstance` machinery and the call
glue.

Concretely:

- `lp-shader/lpvm-emu/src/instance.rs::EmuInstance` aggregate-arg
  marshalling: allocate guest memory in `EmuSharedArena`, write the
  bytes from `LpvmDataQ32`, pass the guest base address as the call
  argument.
- For aggregate returns: take the size from
  `lps_shared::layout::std430` (via the LPIR-level signature, which
  knows the byte size — see #2), allocate an arena buffer, pass its
  address as the hidden first arg, after the call read the bytes back
  into `LpvmDataQ32::from_bytes`.
- The existing `call_function_with_struct_return` (or equivalent) in
  the riscv-emu wrapper continues to drive the guest call; the trigger
  source is now "the LPIR signature has sret" rather than a heuristic.

**Out of scope:**

- Codegen instruction changes — those live in P4 (cranelift).
- WASM marshalling (P7).
- Filetest CHECK rewrites (P9).
- `lpvm_abi.rs` aggregate-arm cleanup (P8).

## Code organization reminders

- Keep changes scoped to `lp-shader/lpvm-emu/src/`.
- Place the aggregate marshalling helpers near `EmuInstance::call`.
- Don't introduce new public API unless needed by tests.

## Sub-agent reminders

- Do **not** commit.
- Stay strictly within `lp-shader/lpvm-emu/`.
- Do **not** suppress warnings or add `#[allow(...)]`.
- Do **not** weaken or `#[ignore]` tests.
- If you discover that `EmuSharedArena` does not expose alloc / write /
  read primitives sufficient for the marshalling described here, **stop
  and report** — don't reach into emu internals.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. Inventory the existing emu call path

Read these files first:

- `lp-shader/lpvm-emu/src/instance.rs` — `EmuInstance::call` (or
  similar). Look at `run_emulator_call` and how it currently handles a
  scalar/sret split.
- `lp-shader/lpvm-emu/src/host_marshal.rs` (or similar) — host-side
  arg/return marshalling helpers if extracted.
- The `EmuSharedArena` API (likely in `lp-riscv/lp-riscv-emu*` —
  search for `pub struct EmuSharedArena` or `pub fn alloc`). You need:
  - alloc N bytes with alignment.
  - write a slice into the allocated region at a given guest address.
  - read N bytes back from a guest address.
- Whatever existing path handles the cranelift "scalar return count
  > N → sret" case. That path is your reference for buffer alloc,
  passing the address through the call, and reading back. The trigger
  source is what changes.

### 2. Aggregate arg marshalling

For each aggregate argument (host-side `LpvmDataQ32`):

```rust
// host pseudo-code; adapt to actual API
let bytes = data.as_bytes();           // LpvmDataQ32 bytes (std430)
let size  = bytes.len() as u32;
let align = data.alignment();          // from LpvmDataQ32 / lps_shared::layout
let guest_addr = arena.alloc(size, align)?;
arena.write(guest_addr, bytes)?;
call_args.push(GuestVal::I32(guest_addr));  // or whatever the call API expects
```

For `out`/`inout` args: alloc + (for `inout`) write + after call read
back into the host's `LpvmDataQ32`. M1 only needs `in` working
correctly; `inout` aggregate args are not produced by the frontend's
M1 lowering for any test, but if the path exists, do not regress it.

### 3. Aggregate return (sret)

If the LPIR signature has `sret_arg.is_some()` (or the import has
`sret == true`):

1. Determine the return buffer size. This must come from the LPIR-level
   abstraction the host already uses. The `IrFunction` / `ImportDecl`
   itself doesn't carry the byte size — but the call-site host code does
   know the host return type (a `LpsType` provided by the caller, since
   the caller is preparing an `LpvmDataQ32` to receive the result).
   Compute size via `lps_shared::layout::type_size(&lps_type,
   LayoutRules::Std430)`.
2. Alloc that many bytes in `EmuSharedArena` with appropriate alignment.
3. Pass the guest address as the hidden first user-arg (after vmctx).
   The existing rv32 ABI hands sret in `a1` (per P5); if the cranelift
   path uses a different register, follow whatever cranelift's sret
   convention does on rv32. Check that P4 / P5 agree on the slot.
4. Run the call.
5. Read the bytes back from the guest address into a `Vec<u8>`, then
   `LpvmDataQ32::from_bytes(lps_type, LayoutRules::Std430, bytes)`.

### 4. Trigger source

Replace any `if scalar_return_count > N` decisions in the emu host path
with `if signature.has_sret()`. If the signature abstraction the host
uses doesn't carry that bit, propagate it from the `IrFunction` /
`ImportDecl` at construction time (e.g. via a `HostCallSig` struct
that records `bool sret` and `Option<u32> sret_size`).

Do not duplicate the heuristic — single source of truth is the LPIR
marker.

### 5. Tests

Add an integration test (or extend an existing one) in `lpvm-emu`:

- Build a tiny LPIR module with one entry function that takes an
  aggregate `in float[4]` and returns `float[4]` (sret).
- Have the entry compute element-wise `in[i] * 2.0` into the sret
  buffer.
- From the host:
  - Create an `LpvmDataQ32` containing `[1.0, 2.0, 3.0, 4.0]`.
  - Allocate a destination `LpvmDataQ32` for `float[4]`.
  - Call the entry; assert the destination contains
    `[2.0, 4.0, 6.0, 8.0]`.

If the test harness doesn't easily support hand-built LPIR, write the
test using a small GLSL source compiled via `lps-frontend`; that is
also fine and exercises the full P3-P4-P6 stack.

## Validate

```
cargo check -p lpvm-emu
cargo test  -p lpvm-emu
just test-glsl
```

Filetest-level emu failures may exist until P9. Report whether failures
are CHECK-line mismatches or genuine marshalling bugs.

## Done when

- `EmuInstance::call` (or equivalent) marshals aggregate args via
  `EmuSharedArena` + `LpvmDataQ32`.
- Aggregate returns alloc an arena buffer, pass it as sret, and read
  back via `LpvmDataQ32::from_bytes`.
- Trigger source for sret is the LPIR marker (no heuristic).
- New round-trip integration test passes.
- `cargo test -p lpvm-emu` is green.
- `just check` is green for this crate.
- No new `#[allow(...)]`; no `#[ignore]`d tests.
