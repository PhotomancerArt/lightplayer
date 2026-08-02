# ADR: License provenance discipline for derived code (Xtensa backend and beyond)

- **Status:** Accepted
- **Date:** 2026-07-29
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Origin:** Mirrors
  `docs/adr/2026-07-28-license-provenance-discipline.md` in the
  ESP32-S3 experiment repo (github.com/PhotomancerArt/2026-esp32s3-experiment),
  written there before any derivation work began. That ADR remains the dated
  record of when the discipline started; this one is its lp2025-scoped
  counterpart and does not restate its history.

## Context

LightPlayer is licensed **AGPL-3.0 by choice, not necessity**. While the
project holds sole copyright it retains the option to relicense later —
dual-licensing, a commercial edition, or something else not yet imagined. That
option survives only as long as provenance is **provable**: which code is
originally Photomancer's, and, for anything derived, exactly where it came from
and under what terms.

Two facts make this a decision that has to be recorded *now*, in this repo:

1. **lp2025 is where outside contributions actually arrive.** The experiment
   repo is a solo workspace; this monorepo is the one with a public surface,
   PRs, and AGPL inbound contributions. Whatever discipline governs derived
   code has to be written down here, not only there.
2. **Derived code is about to land.** The ESP32-S3 (Xtensa) backport brings
   `lp-xt-*` crates — an instruction crate, an emulator, an ELF loader, an
   emitter — whose encoding tables are derived from `espressif/llvm-project`'s
   Xtensa TableGen `.td` files (Apache-2.0 WITH LLVM-exception) and carry
   per-file provenance headers to say so. The seam is described in the
   experiment repo's `BACKPORT.md`.

Building an ISA backend invites copying from existing implementations, and the
richest ones are copyleft: QEMU (GPL-2.0), GNU binutils/GDB (GPL-3.0), and GCC
(GPL-3.0). Copying or transliterating any of them into this repo would attach a
permanent, un-relicensable obligation to the most reusable assets in the
codebase — and would do so invisibly, since a transliterated function looks
like ordinary Rust in review.

## Decision

**1. No GPL source, ever.** No code in this repository may be copied,
transliterated, or line-by-line adapted from a GPL-licensed project. For the
Xtensa work specifically, the named projects and their status:

- `espressif/qemu` (GPL-2.0) — run and observe it as a behavioral oracle; read
  its source to understand semantics; **never reproduce its code**.
- `binutils-gdb` (GPL-3.0), including `xtensa-modules.c` and the linker's
  relocation handlers — same rule. Assembler/objdump *output* is a fact and may
  be used as golden data; the tool's source may not be reproduced.
- GCC (GPL-3.0) — same.

"Behavioral reference" means: observe inputs and outputs, understand the
algorithm in the abstract, then implement independently from primary
specifications. It does **not** permit translating a specific function into
Rust.

**2. Apache-2.0-WITH-LLVM-exception derivation is permitted, with provenance.**
`espressif/llvm-project`'s Xtensa `.td` files are Apache-2.0 WITH
LLVM-exception, which is compatible with relicensing. Encoding *data* (bit
layouts, operand fields, opcode values) may be derived from it, provided:

- each derived file carries a **provenance header** naming the upstream repo,
  path, and commit SHA;
- the upstream license text is **vendored** in this repo under `licenses/`
  (`LLVM-Apache-2.0-with-LLVM-exception.txt`) — the directory arrives with the
  `lp-xt-*` crates, since there is nothing to vendor until then;
- what is derived is factual encoding data, not creative code structure.

This is the general rule for permissively-licensed upstreams, not an
Xtensa-only carve-out: any future derivation from Apache/MIT/BSD sources
carries the same header + vendored-license requirement.

**3. Primary specifications are always safe.** The Xtensa ISA Reference Manual,
the Xtensa ISA Summary, and the ESP32-S3 TRM are the *preferred* sources for
encodings, semantics, and relocation formats — facts, not expression. Prefer
them over any implementation.

**4. Contribution intent.** Outside contributions to code whose relicensing
option matters should be accepted only under a CLA, or a DCO with an explicit
license grant, so the option survives across contributors. Recorded here as
intent; the mechanism gets formalized when the first outside contribution is
proposed.

**5. Enforcement is mechanical, via the agent guides.** `AGENTS.md` restates
rules 1–2 imperatively so automated agents refuse GPL copying by default and
add provenance headers without being asked — the same enforcement seam the
experiment repo uses. Crate READMEs for derived crates carry a Provenance
section. Review of any PR touching `lp-xt-*` should check the headers.

## Consequences

- The Xtensa emulator and instruction crate were implemented from the ISA
  manual + LLVM `.td` data + behavioral diffing against hardware (and
  optionally QEMU) instead of porting QEMU's core. That is slower and it is a
  deliberate, already-paid cost; it must not be "optimized" later by someone
  reaching for the faster source.
- Incoming `lp-xt-*` crates will look unusual in review: per-file provenance
  headers, a `licenses/` directory, golden vectors derived from tool output
  rather than from tool source. All three are load-bearing, not decoration.
- A contributor (human or agent) who cannot say where a piece of code came from
  is a blocker, not a nit. If unsure whether a source is safe to copy from:
  **ask; do not copy.**
- If a future decision abandons the relicensing option, this ADR can be
  superseded and the GPL constraint relaxed — but that is a one-way door and
  must be explicit, taken deliberately, and recorded as its own ADR.

## Alternatives Considered

- **Rely on the experiment repo's ADR alone.** Rejected: that repo has no
  inbound contributions and no PR surface. The rule has to be enforceable where
  the contributions land, and the agent guides that enforce it live here.
- **Port QEMU's Xtensa core** (the fastest route to an emulator). Rejected:
  permanent GPL encumbrance on the single most reusable asset, killing the
  relicensing option outright.
- **Stay AGPL forever and copy freely.** Rejected: forecloses a choice
  Photomancer wants to keep open, in exchange for a short-term implementation
  saving.
- **Case-by-case review instead of a standing rule.** Rejected: provenance is
  cheap to record at authoring time and expensive-to-impossible to reconstruct
  afterwards, which is precisely why this is written before the crates land.

## Follow-ups

- Vendor `licenses/LLVM-Apache-2.0-with-LLVM-exception.txt` when the `lp-xt-*`
  crates land (nothing to vendor before then).
- ~~Formalize the CLA / DCO-with-grant mechanism when the first outside
  contribution to relicensing-sensitive code is proposed.~~ Done ahead of that
  trigger — see `2026-07-31-contributor-license-agreement.md`.

## Addendum (2026-08-01): WLED's license changed, and is not GPL

Several planning docs referring to WLED's ESP32 RMT driver as off-limits
(the classic-ESP32 bring-up roadmap's M5 brief, `lp-fw/lp-ws281x`'s
provenance note) describe it loosely as "WLED's GPL shim." That is imprecise
and, for older code, wrong. Verified from WLED's repo history (via
Ben Hencke) during roadmap M5's `2026-08-01-1459-rmt-priority-hli` plan:

- WLED was **MIT-licensed from 2016-12-28 to 2024-10-15**, then relicensed to
  **EUPL** ("Re-license the WLED project from MIT to EUPL (#4194)").
  Relicensing is not retroactive — revisions before the switch remain MIT.
- The consult-prohibition in this ADR and `AGENTS.md` (no copying,
  transliterating, or line-by-line adapting from copyleft sources) applies
  to WLED's **post-switch EUPL code** the same as it would to GPL: EUPL is
  copyleft, and neither this ADR's decision nor `AGENTS.md`'s enforcement
  distinguishes "which copyleft license" — the rule is "no copyleft source,
  full stop," so the practical prohibition is unchanged by this correction.
- What changes: a **pre-2024-10-15 MIT revision** of WLED's own code (not
  NeoPixelBus, which it may have adapted — NeoPixelBus was historically
  LGPL and needs its own check) is permissively licensed and, per this ADR's
  rule 2, portable with attribution and a provenance header, unlike GPL/EUPL
  source. This does not by itself authorize using it — it opens a path that
  did not previously exist for one specific piece of code.
- The concrete instance: the classic ESP32's level-4/5 high-priority
  interrupt vector, parked NO-GO at M5's G1 gate
  (`2026-08-01-1459-rmt-priority-hli/notes.md`, "G1 COMPLETE"). Its
  documented reopen ladder now runs a step-zero, per-file provenance check
  against WLED's MIT-era history before falling back to pure clean-room from
  Espressif's Apache-licensed `hli_vector.S`.
