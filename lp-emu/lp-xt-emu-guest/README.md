# lp-xt-emu-guest

`no_std` guest runtime for programs running **inside the `lp-xt-emu` emulator**
(not on a board — there is no esp-hal here). Mirrors the module set of
lp2025's `lp-riscv-emu-guest` (`entry` / `syscall` / `print` / `panic` /
`allocator`), adapted to Xtensa.

## Trap mechanism (the guest ↔ host contract)

Guest → host calls use the **`SYSCALL` instruction** (bytes `00 50 00`,
assembler-verified), which `lp-xt-emu` surfaces to a host-installed
`SyscallHandler` (`lp-xt-elf`'s `GuestHost`). Register convention:

| register | meaning |
|----------|---------|
| `a2` | syscall number (`SYS_EXIT`=1, `SYS_WRITE`=2, `SYS_PANIC`=3) |
| `a3..a5` | arguments (ptr/len for write and panic; code for exit) |
| `a2` (on resume) | host result |

The guest wrapper is a tiny windowed assembly function
(`lp_xt_guest_syscall`): a CALL8 into it rotates the Rust arguments
(caller `a10..a13`) into its `a2..a5` — exactly the ABI registers — so no
inline-asm register constraints are needed, and the host's `a2` result rides
back through RETW as the return value.

Host-side constants live in `lp-xt-elf/src/abi.rs`; **keep the two in sync**
(every fixture test exercises the ABI, so drift fails loudly).

## Why there is no startup assembly

- The emulator invokes `_start` via a synthesized windowed CALL8 with a valid
  SP already staged (stack lives in the emulator's dedicated stack region).
- The ELF loader materializes `.data` and zeroes `.bss` directly.

So `_start` is a plain windowed `extern "C" fn(u32) -> u32`, defined by the
`emu_main!` macro around your `fn main(arg: u32) -> u32`; it calls `exit(code)`
(the `SYS_EXIT` trap) when main returns.

## Allocator

A fixed-buffer (16 KiB) bump allocator, deliberately **non-atomic**: the guest
is single-threaded and the emulator has no `s32c1i`, so atomic CAS sequences
must not be emitted. Never frees.
