# ADR: `lp-emu/` is the home of all emulation, and it is MIT behind a lint

- **Status:** Accepted
- **Date:** 2026-09-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-07-28-emu-core-crate-family.md` (created `lp-emu/` as the
  arch-neutral substrate and its neutrality rule);
  `2026-07-29-license-provenance-discipline.md` (why AGPL is a choice and why
  provenance must stay provable);
  `2026-07-31-contributor-license-agreement.md` (what makes relicensing
  possible at all); `docs/reports/2026-07-28-xtensa-monorepo-readiness.md`
  (the precedent this decision is reacting to)

## Context

Emulation had three homes. `lp-emu/` held the arch-neutral substrate
(`lp-emu-core`, `lp-emu-abi`); the RV32 emulator and its guest runtime sat in
`lp-riscv/` next to the instruction model; the Xtensa emulator and its guest
sat in `lp-xt/`. Nothing about that grouping was wrong when each arrived — the
emulator grew out of the ISA crates in both cases — but it means "where does
the emulator live" has three answers, and the ESP32-C6 SoC emulator now being
planned would have added a fourth.

Two forces make this the moment to fix it.

**The ESP work is a large, self-contained body of code that we would like to
be able to share.** An ESP32 emulator that runs a real image on the real
memory map is useful to people who will never touch LightPlayer. The product
is AGPL by choice and stays that way; the emulator does not have to be. But a
licence boundary that exists only in intent is a boundary that leaks on the
first convenient import.

**We have already paid for the alternative.** `lp-xt-emu` was built in a
separate repo (`2026-esp32s3-experiment`) and brought in later; that cost a
readiness report and a backport milestone before a line of it ran here. A
directory with a lint is cheaper than a second repository, and it keeps the
emulator in the same CI, the same review, and the same `just check`.

Vision D2 settled the licence question ("monorepo, fenced MIT"); D9 settled
the shape ("namespaced by vendor, nothing crazy"); Q3 left the ISA crates
where they are. Plan PD1 and PD2 turn those into this change.

## Decision

**All emulation lives under `lp-emu/`, and everything under `lp-emu/` is
MIT, enforced by a lint.**

### The home

| Before | After |
|---|---|
| `lp-emu/lp-emu-core` | unchanged |
| `lp-emu/lp-emu-abi` | unchanged |
| `lp-riscv/lp-riscv-emu` | `lp-emu/lp-riscv-emu` |
| `lp-riscv/lp-riscv-emu-guest` | `lp-emu/lp-riscv-emu-guest` |
| `lp-riscv/lp-riscv-emu-guest-test-app` | `lp-emu/lp-riscv-emu-guest-test-app` |
| `lp-xt/lp-xt-emu` | `lp-emu/lp-xt-emu` |
| `lp-xt/lp-xt-emu-guest` | `lp-emu/lp-xt-emu-guest` |

**Crate names do not change** (PD1). Only paths move, so every `use` in the
repo is untouched and the diff is a rename plus manifests.

**Namespaced by vendor** (D9): architecture cores sit at the root of
`lp-emu/`, because an ISA outlives any one chip. Anything that assumes a
*chip* — memory map, MMIO decode, peripherals, a vendored ROM — goes under a
vendor directory. `lp-emu/esp/` is created empty (with a README) to make the
rule visible before the first SoC crate lands; a Raspberry Pi Zero would get
`lp-emu/rpi/` beside it.

**The ISA and ELF crates stay put** (Q3): `lp-riscv-inst`, `lp-riscv-elf`,
`lp-xt-inst`, `lp-xt-elf`, the `lp-xt-fp-*` pair and the `lps-builtins-xt-*`
pair are compiler-backend and hardware-validation infrastructure, shared with
`lpvm-native`'s codegen and `rt_emu`. They are not emulation, and moving them
would drag the compiler's dependencies through the fence.

### The fence

Every package under `lp-emu/` declares `license = "MIT"`, with one
`lp-emu/LICENSE-MIT` for the unit. No per-crate `license-file` (an SPDX id
makes it unnecessary), and no licence headers in source files — the repo does
not use them, and the existing provenance headers in `lp-xt-emu` are
untouched.

`scripts/check-emu-fence.sh` (`just lint-emu-fence`, in `check-lint`, so in
CI's `Lint (x64)` job) asserts:

1. every package under `lp-emu/` declares exactly `license = "MIT"`;
2. none of them reaches a workspace-local crate outside `lp-emu/`,
   transitively, except the crates in the script's `ALLOWED_OUTSIDE` table.

Two details that matter more than they look:

- **The walk uses declared dependencies, not the resolved graph** — normal,
  dev and build, optional included. A dependency behind a cargo feature is
  still an import, and `lp-riscv-emu-guest`'s `lp-perf` edge is exactly that
  shape.
- **The allowlist names crates, not forbidden directories.** A blacklist of
  product directories would have made "dependencies on the ISA crates are
  permitted" incidentally true rather than deliberate, and would have missed
  two real edges: `lp-recovery` and `lp-perf`, both AGPL, both under
  `lp-base/`, both reached by the rv32 guest runtime.

The permitted edges out of the fence, today:

| Crate | Licence | Why |
|---|---|---|
| `lp-riscv-inst`, `lp-riscv-elf` | AGPL | rv32 ISA / ELF; Q3, see **Open** |
| `lp-xt-inst`, `lp-xt-elf` | AGPL | Xtensa ISA / ELF; Q3, see **Open** |
| `lp-xt-fp-vectors` | AGPL | FP vector corpus, dev-dependency only |
| `lp-recovery` | AGPL | rv32 guest's panic → staged crash record |
| `lp-perf` | AGPL | rv32 guest's free-list hook, optional `profile` |
| `lp-ws281x` | MIT | already MIT; the M5 strip decoder's counterpart |

One edge was **deleted rather than allowlisted**: `lp-riscv-emu-guest`
declared `lpc-shared` and referenced it nowhere, which pulled `lpc-model`,
`lpc-wire`, `lpc-hardware`, `lps-shared`, `lps-q32`, `lpfs` and
`lp-collection` into the guest's graph. That is the fence doing its job on
its first run.

## Consequences

- "Where does the emulator live" has one answer, and the C6 SoC work has a
  home to land in that already says what its licence is.
- The MIT unit is **not yet externally self-contained**: six AGPL crates sit
  on its dependency edges (table above). Anyone extracting `lp-emu/` today
  would have to replace or relicense them. This is a known, listed cost, not
  an accident — the lint is what keeps the list short and honest.
- Adding a dependency to an `lp-emu/` crate now has a cost: either it is
  inside the fence, or it needs a line of justification in the lint. That
  friction is the point.
- CI path filters gained `lp-emu/**` beside `lp-riscv/**` in the shader, gfx,
  browser and firmware gates. `lp-emu/**` had matched no filter at all — a
  gap that predated the move but would have started hiding real regressions
  the moment `lp-riscv-emu` left `lp-riscv/`.
- `lp-xt-emu-guest` stays an out-of-tree member of the `lp-xt/fixtures`
  esp-toolchain workspace across the move. Root `cargo metadata` cannot see
  it and running cargo in that directory would pull the esp toolchain in CI,
  so the lint checks it at the manifest level instead — MIT declared, and no
  path dependency (it has none).
- Dated plans and reports under `docs/plans/`, `docs/reports/`,
  `docs/design/` and `docs-archive/` keep the old paths on purpose; they are
  records of work as it was. Living docs (ADRs, the defect register, the heap
  budget gate) were repointed.

## Alternatives Considered

**A separate repository for the emulator.** Rejected on measured cost:
`lp-xt-emu` was built that way and needed a readiness report plus a backport
milestone before it ran here. A subtree split stays available later, and the
fence is what would make it cheap — it is the same boundary check either way.

**Per-crate MIT with no lint.** Rejected. `lp-fw/lp-ws281x` is the precedent
for a per-crate MIT id in this workspace, and it works because nothing
depends *out* of it. An emulator family with seven crates and a growing SoC
layer will acquire an AGPL import the first time one is convenient, and
nothing would say so. The lint found a live one (`lpc-shared`) on its first
run, before it had ever guarded anything.

**Move the ISA and ELF crates under `lp-emu/` too.** Rejected here, and
genuinely open (below). They are shared with `lpvm-native`, which is product
code compiled into firmware; putting the compiler's instruction model inside
an MIT fence either relicenses a compiler backend or fills the allowlist with
edges pointing the other way.

**A directory blacklist instead of a crate allowlist.** Rejected: see the
fence section. The blacklist is easier to write and misses the edges that
matter.

## Open

**E1 — should the ISA and ELF crates flip to MIT as well?** The four crates
`lp-riscv-inst`, `lp-riscv-elf`, `lp-xt-inst`, `lp-xt-elf` (and, on the same
argument, `lp-xt-fp-vectors`) are the only reason the MIT unit is not
self-contained. Flipping them would make `lp-emu/` extractable as it stands;
leaving them AGPL means an external consumer must supply their own
instruction model, which for an emulator is most of the interesting part.
Against flipping: they are compiler-backend crates used by `lpvm-native`, so
the licence line would then run through the middle of the compiler rather
than around the emulator.

Raised at gate G1 alongside a second, smaller version of the same question:
`lp-recovery` and `lp-perf`, which the rv32 guest runtime reaches for its
panic path and its profiling hook. Both are small and both are AGPL.
