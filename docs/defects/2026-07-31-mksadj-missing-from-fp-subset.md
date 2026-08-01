---
status: fixed
found: 2026-07-31      # how: M6 P6 hardware campaign (sequence transcription)
fixed: this change
area: lp-xt/lp-xt-inst
class: incomplete-subset
related:
  - docs/adr/2026-07-31-xtensa-fp-behavior-contract.md
---
# `mksadj.s` was recorded as an unassigned slot, so the sqrt sequence could not decode

**Symptom** — Transcribing the toolchain's real square-root sequence
(`__ieee754_sqrtf`, esp-14.2.0 libm, read via `xtensa-esp32s3-elf-objdump`)
for the M6 P6 campaign hit an instruction the P1 normative subset did not
contain: `0xfa21c0`, which objdump disassembles as `mksadj.s f2, f1`. The
emulator could not execute the sequence at all — every square root would have
raised an unsupported-instruction trap at its 23rd instruction.

**Root cause** — `lp-xt-inst`'s FP1 unary decode table carried a comment (and
a test, `fp1_unassigned_slots_stay_unsupported`) asserting that selector
`t = 0xC` "has no mnemonic in `xtensa-esp32s3-elf-objdump`". That was wrong:
objdump assigns it `mksadj.s`, the square-root counterpart of `mkdadj.s`. The
P1 probe list was assembled from the divide-sequence shape and never included
a square-root sequence, so the gap survived P1's silicon session — presence
was probed for 26 instructions and `mksadj.s` was not one of them.

**Why it matters beyond the campaign** — M7's square-root lowering emits this
sequence. Without the fix, `sqrt()` in any shader would have been
un-emulatable, discovered only when M8's filetests first ran a sqrt on the
`xtn` target.

**Fix** — `MksadjS` added to `Inst::FpRr` (decode `t = 0xC`, encode, disasm,
roundtrip, golden vector `0xc0 0x21 0xfa`); the emulator classifies it with
the divide-step family and implements its measured semantics
(`fp_rom::mksadj`: `A = ⌊(e − 127)/2⌋` split-encoded, class codes for
zero/negative/inf/NaN, INVALID on negative or signalling input). The
wrongly-asserting test now pins only `t = 0x2` as unassigned.

**Signal** — an "unassigned per objdump" claim is only as good as the objdump
invocation behind it; the claim had no golden pinning it to a real objdump
run. The campaign's rule — transcribe real toolchain output, never enumerate
from memory — is what caught it.
