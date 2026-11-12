# ADR: Host-shared guest memory for the Xtensa emulator

- **Status:** Accepted (amended 2026-08-01 — the address changed, the decision did not)
- **Date:** 2026-07-30
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

> **Amendment, 2026-08-01.** `SHARED_DBUS_BASE` is now **`0x3000_0000`**, not
> `0x3F40_0000`. This ADR's own reasoning predicted it: "the assertion, not the
> address, is the decision that matters. A comment claiming an address is free
> rots the moment a profile changes." Both profiles gained modeled flash windows
> and `0x3F40_0000` turned out to *be* classic's DROM base (esp-hal
> `ld/esp32/memory.x`, `drom_seg`). The `add_shared` assertion failed at the
> moment of the mistake, exactly as designed. The replacement is chosen on
> stronger ground than "no profile maps it": `0x3000_0000` is below the lowest
> address either chip's data bus decodes, so no future profile can reach it.
> Everything else below stands as written; read `0x3F40_0000` as the historical
> value.

## Context

The host-side shader execution path (`lpvm-native`'s `rt_emu`) does not copy data
in and out of the guest. It allocates the shader's vmctx — uniforms, globals, the
snapshot region, texture buffers — from an `EmuSharedArena`
(`Arc<Mutex<Vec<u8>>>`), maps that same allocation into the guest's address
space, and hands the shader a pointer to it. Guest stores land in host bytes;
every host-side read-back in `rt_emu/instance.rs` goes through the arena rather
than through the emulator.

For rv32 this is `lpvm_emu::Memory::new_with_shared(...)`, which places the arena
at `lp_emu_core::DEFAULT_SHARED_START` (`0x4000_0000`) — an address chosen
because nothing else in the rv32 guest map uses it (code at 0, RAM at
`0x8000_0000`).

`lp-xt-emu` had no equivalent. Its `Memory` models a set of `Vec`-backed regions
with per-region D-bus/I-bus aliasing — an accurate model of SRAM1 on the S3 and
classic ESP32 — and nothing that maps host-owned bytes. Registering Xtensa
filetest targets therefore required deciding how a host buffer reaches a guest
address space whose *real silicon map has no such region*: on device, vmctx lives
in ordinary DRAM and the on-device JIT (`rt_jit`) never needs a construct like
this.

Copying the vmctx in and out around each call was considered and rejected on
correctness, not cost: globals persist across calls, the snapshot/reset path
memcpys *within* guest memory, textures are large and read lazily, and one arena
is shared across instances.

## Decision

`lp-xt-emu::Memory` gains an optional host-shared window, attached with
`add_shared(dbus_start, Arc<Mutex<Vec<u8>>>)`, with four properties:

1. **One field on `Memory`, not a `Region` variant.** Mirrors `lp-emu-core`'s
   `shared_backing`. The region list, its `AliasRule` machinery, and the fetch
   path stay untouched.
2. **Data only — never fetchable.** The window carries no `AliasRule`, so an
   instruction fetch from it takes the ordinary `EXC_INSTR_FETCH_ERROR` path.
   Jumping into the vmctx is a bug and the emulator models it as one.
3. **One lock per typed access**, not per byte and not held across a run — the
   granularity `lp-emu-core` already uses for its shared backing.
4. **A dedicated base address, `SHARED_DBUS_BASE = 0x3F40_0000`**, *not* the
   rv32 engine's `DEFAULT_SHARED_START`, plus a **runtime overlap assertion** in
   `add_shared` against every installed region — both its D-bus range and its
   I-bus image.

The two ISAs therefore use different shared bases. That costs nothing: guest code
reaches the region only through a pointer argument, so no emitted instruction
encodes the base.

### Why not reuse `DEFAULT_SHARED_START`

Reuse was the initial plan — one constant, `EmuSharedArena` untouched. It is
wrong: `0x4000_0000` is `lp_xt_emu::SENTINEL_PC`, the deliberately-unmapped
return address the windowed run harness detects a top-level return with. Mapping
the shared window there would place the vmctx at the sentinel and quietly
undermine the "chosen unmapped" property that harness depends on.

`0x3F40_0000` is in the external-memory-mapped range on both chips, which no
board profile models, and outside the `0x4xxx_xxxx` I-bus quadrant every profile
executes in. The S3 installs SRAM1 code/stack at `0x3FC8_8000`/`0x3FCC_0000`
(I-bus images from `0x4037_8000`); classic installs `0x3FFE_8000`/`0x3FFC_0000`
(I-bus image `0x400A_1000..0x400B_8000`).

The assertion, not the address, is the decision that matters. A comment claiming
an address is free rots the moment a profile changes; the assert fails loudly at
the moment of the mistake, and a test attaches the window on both profiles.

### This address is host-emulator fiction

Stated explicitly so nobody later mistakes it for a hardware fact: no ESP32 has a
host-shared region. `SHARED_DBUS_BASE` exists only so a host engine can hand
guest code a pointer into host memory. Device code paths must never reference it.

## Consequences

- `rt_emu` can run Xtensa shaders with the *same* vmctx/uniform/global/texture
  plumbing as rv32; that plumbing stays genuinely ISA-neutral.
- The two shared-base constants are per-ISA, so anything reading
  `DEFAULT_SHARED_START` for a non-rv32 guest is a bug. The arena's base is a
  field (`EmuSharedArena::shared_start`), so a host engine must construct the
  arena with the base its ISA's emulator expects.
- A third ISA repeats this: pick a base unmapped in *that* guest map, check it
  against the ISA's sentinel/reserved addresses, and let `add_shared`'s assert
  enforce it.
- Locking per typed access means a texture-heavy shader takes many uncontended
  locks. Matches rv32, so filetest cost is comparable; if it ever shows up in a
  profile, the fix is a longer-held guard, not a different memory model.
- `Memory::load_bytes` now writes into the shared window when the address falls
  in it, so loader-style setup works there too.

## Alternatives Considered

- **Copy the vmctx in and out per call.** Rejected on correctness: persistent
  globals, in-guest snapshot/reset memcpys, and lazily-read textures all assume
  one shared buffer.
- **Reuse `DEFAULT_SHARED_START` (0x4000_0000).** Rejected: collides with
  `SENTINEL_PC` (above).
- **Move `SENTINEL_PC` instead.** Rejected: the sentinel is chosen so RETW's
  address-unmangle reproduces it exactly, and the board profiles document their
  maps against it. Moving a verified constant to accommodate a host-only fiction
  is the wrong direction of accommodation.
- **A `Backing::{Owned,Shared}` enum inside `Region`.** Rejected: it would push a
  lock into every access path including fetch, for a window that must never be
  fetchable.
- **Builtins as emulator syscalls instead of guest code** (the wider "Option B"
  from the Xtensa roadmap). Already decided against — the cross-compiled builtins
  image keeps the host path faithful to the device path.

## Follow-ups

- `lpvm-native`'s `rt_emu` constructs the arena at this base for the Xtensa arm
  (next phase of the Xtensa filetest plan).
- If a measured Xtensa cycle model lands, shared-window accesses are one of the
  classes worth costing separately.
