# ADR: Force a register-window spill before walking an Xtensa backtrace

- **Status:** Accepted
- **Date:** 2026-07-30
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

`lpc_shared::backtrace::capture_frames` is the frame walker every LightPlayer
panic path uses. On riscv32 (ESP32-C6) it is a plain frame-pointer chase: `s0`
points at a frame, `[fp-4]` is the return address, `[fp-8]` is the caller's
frame pointer, and every candidate is bounds-checked against the C6's DRAM
window. The Xtensa arm was a 0-frame placeholder, and the ESP32-S3's abort-tier
crash report said so in as many words rather than pretending the stack was
empty.

Xtensa's windowed ABI does not give the same walk for free, and the difference
is not a detail of encoding — it is a difference in *where the data is*.

A frame's return address (`a0`) and stack pointer (`a1`) live in the physical
register file, not in memory. The processor holds a 64-entry register file that
software sees through a rotating 16-register window; nothing writes a frame's
`a0`/`a1` to the stack until `ENTRY` detects that the window ring has wrapped
onto a live frame and raises a window-overflow exception. So at any moment the
innermost several frames are **not in memory at all**.

That produces a specific and dangerous failure mode. A memory walk that skips
the spill still returns addresses — it reads whatever the last deeper call
chain left below those stack pointers. Those are real code addresses, they pass
any bounds check, and they symbolize to real function names. The result is a
backtrace that looks entirely plausible and is wrong, which is worse than no
backtrace at all, because a wrong one gets believed and sends someone hunting
the wrong bug.

This was measured, not assumed. The hardware harness runs the walk with the
spill skipped as a control: a call chain of known depth 25 reported a run of 19
identical PCs instead of 25 — **six believable, correctly-typed, wrong
addresses**.

## Decision

The Xtensa arm of `capture_frames_arch` forces every live register window out
to memory before it reads any, and it does so by nesting sixteen ordinary
windowed calls rather than by hand-writing a spill routine.

**The spill mechanism.** `ENTRY` — and only `ENTRY`/`RETW` — performs the
window overflow/underflow check. Sixteen nested calls therefore sweep
`WindowBase` through the S3's entire 16-unit ring even in the worst case where
every call is a `call4` (one unit each), and the hardware's own overflow
handlers necessarily write every previously-live frame to its save area on the
way. Unwinding back out reloads the registers but does not erase the memory
copies, so the chain is intact when the recursion returns. Measured codegen
uses `call8` (two units per level), so sixteen levels is roughly 2× headroom;
the cost is about 800 bytes of stack, spent once, on a path that is already
resetting the chip.

`core::hint::black_box` on both the argument and the result of the recursion is
load-bearing, not defensive: without it LLVM rewrites the accumulator recursion
into a loop and **no window rotation happens at all**. That is a silent failure
— the walk still returns frames, just wrong ones — so the disassembly was
checked (`entry a1, 48` + a non-tail `callx8` to itself) and the on-device
oracle would catch a regression.

**The walk.** From the capturing frame's live `a0`/`a1`:

- The reported PC comes from `a0`, whose top two bits carry the call increment
  rather than address bits; the region bits are restored from the fact that
  everything the S3 executes lives in region 1. `3` is subtracted so
  `addr2line` lands on the `CALLn` rather than on whatever follows it.
- `[sp-16]` is the caller's `a0` and `[sp-12]` is the caller's `a1` — the
  16-byte **base save area** every window-overflow width (4, 8 and 12) writes,
  which is why the chain is uniform regardless of how a frame was entered.
- Both halves are validated before either is used. PCs must be in IRAM
  (`0x4037_0000..0x403E_0000`) or the flash cache window
  (`0x4200_0000..0x4400_0000`); stack pointers must be 16-aligned and inside
  internal SRAM (`0x3FC8_8000..0x3FD0_0000`) with room for `[sp-16, sp)`; and
  each hop must move strictly *up* the stack. A save area whose stack-pointer
  half is garbage is not a save area, so the return address next to it is never
  reported.

**Internal ROM is deliberately excluded** from the executable window. No frame
in this firmware returns into ROM, accepting it would widen the
garbage-looks-valid space by 512 KB, and — because restoring the region bits
turns a zeroed `a0` into `0x4000_0000` — accepting ROM would make the chain
terminator look like a real frame.

**The oracle is as much the decision as the walker.** The walk is split into a
pure chain walk and a tiny target-only prologue that supplies the live
registers, specifically so the chain walk can be driven from host unit tests
against synthetic stacks laid out inside the real S3 DRAM window. On silicon,
a recursive `chain(n)` produces `n` frames that all return to the same call
site, so a correct walk must contain a run of **exactly** `n` identical PCs —
a number fixed by the source rather than by anything the walker reports, and
unaffected by whether `capture_frames` inlines.

## Consequences

- The S3 crash report carries frames. `FRAME_WALKER_PRESENT` in
  `recovery/panic_path.rs` flips to `true`, and the "nothing looked at the
  stack" wording is replaced by wording that distinguishes *the walk found
  nothing* from *there was no walk*. That distinction must be preserved: if a
  future chip lands without a walker, the constant goes back to `false`.
- The address windows are **ESP32-S3 facts baked into a chip-agnostic crate**,
  exactly as the riscv32 arm bakes in C6 DRAM. The classic ESP32 (LX6) shares
  the ABI but not the map; adding it means a second set of constants, not a
  second walk.
- Stacks in PSRAM are not walkable. Nothing in this firmware puts one there,
  and widening the window would only enlarge the space in which garbage can
  look valid.
- `lpc-shared` now enables `asm_experimental_arch` when — and only when —
  targeting Xtensa. Every other build stays on stable features.
- The panic path costs ~800 bytes of stack it did not before. On a stack
  overflow that makes a bad situation marginally worse; the abort tier resets
  either way.
- Decoding S3 frames needs its own recipe (`just decode-backtrace-esp32s3`).
  The C6 and the S3 both place flash text at `0x42xxxxxx`, so one shared
  recipe would symbolize against the wrong image and be confidently wrong —
  the same class of failure as the walk this ADR is about.

## Alternatives Considered

**A `ROTW`-based spill sequence.** Shorter than a nested recursion and what
`esp-backtrace` uses. Rejected on two grounds: it would have been a
transliteration of another project's instruction sequence rather than
independent work, and — more decisively — the mechanism by which it forces a
spill is not something the Xtensa ISA Reference Manual's `ROTW` description
accounts for on its own. Nesting `ENTRY` uses the one primitive whose overflow
semantics the manual states outright, and which `lp-xt-emu` already models.

**Reconstructing live frames from the register file** via `WINDOWSTART` /
`WINDOWBASE` instead of spilling. Needs privileged register reads and window
rotation to reach arbitrary `AR`s, which is considerably more machinery than
forcing the hardware to do the write for us.

**Calling the ROM's `xthal_window_spill`.** Depends on a linker script
providing ROM symbols, which is an esp-hal build detail rather than a property
of `lpc-shared`, and would break any other Xtensa consumer of the crate.

**Emulating the oracle on `lp-xt-emu` instead of silicon.** Preferred by the
phase brief for iteration speed and not taken: `lpc-shared` cannot be built as
an emulator guest (it pulls `lpc-model`, `lpfs`, `serde`), so an emulator
oracle would have had to run a *re-implementation* of the walk. That tests a
copy. The host-side split above gets the fast-iteration benefit against the
real code, and silicon supplies the evidence about the spill — which is the
part an emulator that models spill placement by construction could not have
provided anyway.

**Shipping nothing.** The stated honest outcome if the walk could not be made
reliable. Not needed: `run == depth` holds exactly at three depths on silicon.

## Follow-ups

- Classic ESP32 (LX6) would need its own address windows before this walk is
  correct there. Until then the arm is S3-specific despite the `target_arch`
  gate being chip-agnostic.
- An exception-frame walker (crashes that arrive through the exception vector
  rather than through `panic!`) can reuse `walk_frames_from`, which takes an
  explicit `(ra, sp)` pair for exactly that reason.

## Provenance

No GPL source was read or adapted. The windowed-ABI facts above come from the
Xtensa ISA Reference Manual's Windowed Register Option and the ESP32-S3 TRM's
address map, cross-checked against `lp-xt-emu`'s own window machinery
(`lp-xt/lp-xt-emu/src/executor/window.rs`), which is LightPlayer code that was
dual-run against S3 silicon to depth 100 during the Xtensa backport spike.
`esp-backtrace` (MIT/Apache-2.0) was read as a behavioral cross-check on the
save-area offsets after the layout had been derived; no code was copied from
it, and the spill mechanism chosen here is deliberately not the one it uses.
See `docs/adr/2026-07-29-license-provenance-discipline.md`.
