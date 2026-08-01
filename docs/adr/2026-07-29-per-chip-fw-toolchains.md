# ADR: Per-chip firmware crates own their toolchain and panic strategy

- **Status:** Accepted
- **Date:** 2026-07-29
- **Deciders:** Photomancer
- **Supersedes:** None (narrows the "Alternatives considered" rejection in
  `docs/toolchain-notes.md` — see Context)
- **Superseded by:** None

## Context

The ESP32-S3 (Xtensa) port is happening: the standalone core work
(instruction model, emulator, ELF loading, emitter prototype) is proven on
hardware in the experiment repo
(github.com/PhotomancerArt/2026-esp32s3-experiment) and will be backported.
Two facts collide with current repo policy:

1. **Xtensa has no upstream Rust target.** Building for the S3 requires the
   Espressif rustc fork (espup, `channel = "esp"`, currently a ~1.88-nightly
   derivative). There is no route onto the workspace's pinned nightly.
2. `docs/toolchain-notes.md` pins one nightly in three places and records
   per-crate toolchain overrides as considered-and-rejected. That rejection
   was about isolating the *rv32* firmware crates from the host toolchain —
   maintenance cost vs. nightly-regression risk — not about a target that has
   no alternative toolchain at all.

Separately, the S3 firmware cannot reuse the C6's panic posture. fw-esp32's
OOM/panic recovery rides `panic=unwind` + the `unwinding` crate + the
`__eh_frame` linker-script surgery in `lp-fw/fw-esp32/build.rs`, all coupled
to the pinned nightly's `core::intrinsics::catch_unwind` ABI. `unwinding` has
no verified Xtensa support, and windowed-ABI unwinding is a materially
different machine (window-mangled return addresses). The 2026-07-28 spike
validated the alternative on hardware: abort-tier recovery (custom panic
handler → RTC-fast-RAM blame ledger with unmangled PCs → software reset →
report on next boot), with setjmp/longjmp as the fuel-trap escape.

## Decision

1. **One firmware crate per SOC, each owning its `rust-toolchain.toml`.**
   - The rv32 crates (`fw-esp32` and its future `fw-esp32c6` rename, `fw-emu`,
     `lp-riscv-emu-guest-test-app`) stay on the shared pinned nightly. The
     toolchain-notes rationale stands for them unchanged: one bump surface,
     `unwinding` ABI coupling exercised by `clippy-fw-esp32` as the canary.
   - Future Xtensa crates (`fw-esp32s3`) carry `channel = "esp"` in their own
     `rust-toolchain.toml`, quarantined per-crate exactly like the experiment
     repo's `fw/` split. This is the first second-toolchain in the repo and it
     is limited to targets where no upstream toolchain exists.
2. **Per-chip panic/recovery strategy.** The recovery *core* is the
   arch-neutral `lp-base/lp-recovery`; each chip crate owns its backend glue
   (today `fw-esp32/src/recovery/`: RTC region, reset-cause map, watchdog)
   AND its panic strategy. Concretely:
   - C6: `panic=unwind` + `unwinding` + the `__eh_frame` build.rs patching —
     all of it stays chip-crate-local and must not migrate into the future
     `fw-esp32-common` layer.
   - S3: abort-tier (`panic=abort` + blame ledger + longjmp fuel-escape), per
     the spike. Unwind-parity on Xtensa is deferred, not rejected — revisit if
     `unwinding` grows windowed-ABI support.

## Consequences

- `scripts/bump-nightly.sh` must skip esp-channel toolchain files or its
  rewrite-assert aborts (guard lands with the Lane A lp-emu-core PR).
- CI: an Xtensa job cannot share the pinned-nightly toolchain step; it
  installs espup (the commented `build-esp32-arm` job in `pre-merge.yml` is
  the precedent) and caches `~/.espressif` separately. rust-cache namespaces
  split naturally on the fork's rustc version string.
- The future `fw-esp32-common` crate must build under BOTH toolchains: no
  `unwinding` dependency, no panic-strategy assumptions, no
  toolchain-version-coupled intrinsics.
- Chip-feature unification across crates (a single crate with `esp32c6` /
  `esp32s3` features) is permanently off the table — the toolchain split
  forces separate crates, which is also why the `esp32c6` cargo feature is
  not required to become a real chip selector.

## Alternatives Considered

- **Single toolchain for everything**: impossible; no upstream Xtensa target.
- **Move everything to the esp fork**: regresses the rv32 crates onto a
  slower-moving fork and re-couples `unwinding` to a toolchain we don't
  control; rejected.
- **Unwind-based recovery on S3**: `unwinding` support is unverified/absent
  for Xtensa windowed ABI; abort-tier is hardware-validated today. Deferred.
- **Keeping the per-crate-override rejection absolute** (status quo): blocks
  the S3 port entirely.

## Follow-ups

- `fw-esp32s3` crate + justfile/CI wiring at backport time (separate plan).
- Mirror/reference the experiment repo's license-provenance ADR when the
  `lp-xt-*` crates land.
