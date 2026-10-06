# lp-seam — the emulator seam ABI

An **emulator seam** is a named place in the one shipped firmware where the
emulator may answer instead of running the real code. On silicon the real
body always runs. This crate is the contract both sides compile against:
the declarations, their identity, the descriptor table the firmware carries
and the emulator reads, and the generators that keep a seam alive through a
release build. `no_std`, no dependencies, **MIT**.

Why seams exist and what they may claim is the ADR
[`docs/adr/2026-10-05-emulator-seams.md`](../../docs/adr/2026-10-05-emulator-seams.md).

## Kinds and shapes

| Kind | For | On |
|---|---|---|
| `Performance` | hardware the emulator models faithfully but slowly (the LED output) | only on Studio's Devices-page emulated boards; never in testing, never in `validate record` |
| `Capability` | hardware the emulator cannot model (Wi-Fi, Bluetooth) | every emulated run, once one exists |

| Shape | How the firmware is built around it |
|---|---|
| `Replace` | one real function; the emulator patches its entry and answers |
| `Switch` | an engaged byte, checked at start-up, plugs in a seam-backed adapter |

## Declaring a seam

Every seam is declared once, in the one `declare!` invocation in
`src/lib.rs`:

```rust
seam 0x0001 ws281x_wait_step {
    kind: Performance,
    shape: Replace,
    signature: fn() -> (),
    doc: "…what silicon does, and what an engaged answer does…",
}
```

The rules, checked when the macro expands (a broken one is a build error):

- ids and their body hints are unique;
- `doc:` is a string literal, never `///` (see Identity);
- test seams live in `0x7F00..=0x7FFF`, are named `test_*`, and their doc
  starts `TEST ONLY: never in a shipped table`. Only a harness image carries
  one; an emulator answers one only with its dev feature `test-seams`.

## Identity, and no cross-build compatibility

`SEAM_ABI_ID` is FNV-1a 64 over the `declare!` invocation's tokens with
whitespace (and string line continuations) removed. Comments do not count;
`doc:` text does, because a change to what a seam means is a change to the
ABI. The firmware writes it into its table, and the emulator engages seams
only on an **exact** match. **There is no cross-build compatibility**: an
image from different declarations runs with no seams engaged, and the
emulator says so.

## The descriptor table

Little-endian. Only the magic and the identity are at fixed offsets; the
rest is the identity's to define, and a reader whose identity differs reads
nothing past it.

```text
 0  magic      [u8; 16]   "\xa5LPSEAM-TABLE\x5a\xc3\x3c"
16  abi        u64        SEAM_ABI_ID
24  version    [u8; 32]   LP_APP_VERSION, NUL-padded (diagnostics)
56  self       u32        the table's own address
60  count      u32
64  pending    u32        wake pending word's address, 0 = no wake
68  reserved   u32        0
72  entries    [entry; count], 12 B each:
      +0 id u16, +2 kind u8, +3 shape u8,
      +4 function u32, +8 engaged u32 (0 = no engaged byte)
```

The firmware builds it as a `#[used]` static (`table::SeamTable`, layout
asserted on 32-bit targets) that names its own address. The emulator scans
the **flash chip's bytes** for the magic (the Studio tab has no ELF) and
reads through `table::read`. `self` is how it tells the live table from a
stale one when an update left two cores in flash: the live one's own address
translates, through the running cache MMU, to the offset it was found at.

## Seam functions and call shims: the LTO rule

Firmware never hand-writes a seam function or its call. `seam_fn!`
generates both, one seam per module:

```rust
// seams/test_echo.rs
lp_seam::seam_fn! {
    test_echo => fn(a: u32, b: u32, c: u32) -> u32 { a ^ b ^ c }
}
// call site: seams::test_echo::call(1, 2, 4)
// table entry: SeamEntry::new(&test_echo::DECL, test_echo::ADDRESS, Addr::NONE)
```

- **The seam function** is `#[inline(never)] extern "C"`, exported as
  `lp_seam_<name>`, and holds a non-`pure` `addi zero, zero, <hint>` with an
  immediate unique per seam: LLVM cannot delete the body or fold two into
  one.
- **The call shim** (`call`) reaches it only from an `asm!` block — `call`
  with the arguments bound to `a0..a7`, the result read from `a0`, and
  `clobber_abi("C")` — so LTO cannot internalise the ABI, drop the argument
  set-up, or constant-fold the result. The M0 spike watched an ordinary
  `#[no_mangle]` body lose all three under `lto = true` + `opt-level = "z"`.

Off `riscv32` (host tests, the Xtensa firmwares' builds of shared code) the
shim calls the silicon body directly and no seam function exists. Xtensa
seams are the roadmap's M7.

## The engaged byte

A switch-shape seam's "is it on?" check:

```rust
lp_seam::engaged_byte!(test_take);
// at start-up: if seams::test_take::engaged() { … plug in the adapter … }
```

A `#[used]` `u8` in flash `.rodata`, exported `LP_SEAM_ENGAGED_<name>`, `0`
in the image and read with `read_volatile`. An emulator that engaged the
seam patches it to `1` in the cache window, exactly as it arms code. On
silicon: one load, one branch.

## The wake

One pending word (the table's `pending`) and one line, `FROM_CPU_INTR3` at
priority 1 (`wake.rs`). The host sets bits, then raises; the guest's handler
clears the line, then swaps the word to zero. Whatever a seam wakes runs on
the firmware's IO thread, and the emulator paces what it raises. The C6
image carries the handler since the network seam (`pending` names a RAM
word; bit 0 = frames, bit 1 = station events, any bit wakes both of
`lp-net`'s waiters), bound only when that seam is engaged.

## Licence and the fence

MIT, and outside `lp-emu/` on purpose: the AGPL firmware and the MIT
emulator both depend on it, and `just lint-emu-fence` admits a workspace
crate that declares MIT. Keep it dependency-free.
