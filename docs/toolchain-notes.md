# Toolchain Notes

The workspace uses **nightly Rust** (`rust-toolchain.toml`). This is required by the
bare-metal firmware targets; host crates compile fine on stable but share the workspace
toolchain for simplicity.

## The second toolchain: Espressif's Xtensa fork

Xtensa has no upstream Rust target, so anything built for ESP32-S3 / classic
ESP32 uses Espressif's fork, installed as the `esp` rustup channel (`espup
install`). Two directory trees select it locally: `lp-fw/fw-esp32s3` and
`lp-xt/fixtures` each carry their own `rust-toolchain.toml`. CI pins it via
`esp-rs/xtensa-toolchain` (see `pre-merge.yml`; keep the version in sync across
the `firmware-xtensa` and `validate-xtensa` jobs).

**It is needed for a *host* test path, not only firmware.** The Xtensa filetest
targets (`xtn.q32` / `xtlpn.q32`) and `just test-xt-host` execute Xtensa code on
the host emulator, linked against a cross-compiled builtins image built by
`scripts/build-builtins-xt.sh`.

**Without the esp toolchain everything still builds and tests.** The image is a
gitignored artifact; when it is absent the Xtensa targets and tests skip with a
loud note naming the build command. That is deliberate — `just check` and
`just test` must work on a machine that has never run espup. The corollary is
that those tests can pass vacuously, which is why CI's `validate-xtensa` job
asserts the image is non-empty before running them.

Requires esp Rust **≥ 1.90** for the workspace MSRV: on 1.88 `lpc-model`
genuinely fails to compile (70× E0716 from the `Slotted` derive's
const-promotion of a temporary). `scripts/build-builtins-xt.sh` fails loudly and
names `espup update` rather than working around it.

## Why nightly

Three unstable features are used by `fw-esp32c6` and `fw-emu`:

1. **`-Zbuild-std`** — Rebuilds `core` and `alloc` from source with the flash-budget
   flags below (`optimize_for_size`, `compiler-builtins-mem`). Configured in
   `fw-esp32c6/.cargo/config.toml`.

2. **`#![feature(alloc_error_handler)]`** — A custom OOM handler, so an allocation
   failure is reported with the heap counters attached (requested/free/used/
   `largest_free`/`retry_ok`) instead of arriving through Rust's default handler as
   a bare "memory allocation of N bytes failed". See
   `fw-esp32c6/src/recovery/panic_path.rs`.

3. **`-Zlocation-detail=none` / `-Zfmt-debug=none`** — Flash-budget flags, worth
   ~155 KB. See `docs/adr/2026-07-28-esp32c6-flash-budget.md`.

A fourth used to be here — `#[lang = eh_personality]`, provided by the `unwinding`
crate — and it was the reason the other three existed in the shape they did. All
firmware targets are abort tier now
(`docs/adr/2026-08-02-rv32-firmwares-are-abort-tier.md`), so nothing in the tree
implements a personality routine or rebuilds `core` for `panic = "unwind"`.

## Why the nightly is pinned

The toolchain is pinned to a dated nightly (e.g. `nightly-2026-04-27`), **not** a
rolling `nightly`. The pin lives in three places that must stay in sync:

- `rust-toolchain.toml` (workspace root) — drives local dev and any in-repo
  `cargo`/`rustc` call.
- `lp-fw/fw-esp32c6/rust-toolchain.toml` — a per-crate pin. Several recipes `cd` into
  `lp-fw/fw-esp32c6`, so *this* file wins there; it also carries `rust-src` for
  `-Zbuild-std`. If it drifts from the root pin, the firmware build resolves a
  different (possibly unpinned) toolchain — which is exactly how CI broke once: the
  root was pinned but this file still said `nightly`, so the build-std step ran on a
  rolling nightly with no `rust-src`.
- `.github/workflows/pre-merge.yml` — the `dtolnay/rust-toolchain` step (CI checks
  out into a subdirectory, so the action can't auto-read the toml; the date is
  passed explicitly).

The reason it is pinned rather than rolling is now the ordinary one: reproducibility.
A dated nightly means local `just check` and CI see the same clippy set, so a green
local run is a real signal, and `-Z` flag behaviour does not shift underneath the
flash budget.

### It used to be pinned for a much sharper reason

Until 2026-08-02 the pin was **ABI-coupled**. The
[`unwinding`](https://crates.io/crates/unwinding) crate provided our
`eh_personality` and was bound to the nightly `core::intrinsics::catch_unwind` ABI,
which changed its return type from an integer to `bool`:

- `unwinding` **0.2.8** expects the integer form (`catch_unwind(...) == 0`).
- `unwinding` **0.2.9** expects the `bool` form (`if catch_unwind(...) { ... }`).

There was no single `unwinding` that built on both an old and a new nightly, so the
crate version and the toolchain date were a matched pair, and drifting produced
`E0308: expected bool, found integer` inside a build-std compile. That is why
`bump-nightly` carried a speculative-bump-and-revert search.

`unwinding` is gone with the unwind tier
(`docs/adr/2026-08-02-rv32-firmwares-are-abort-tier.md`). Nothing in the tree is
coupled to a nightly's internal ABI any more, and a bump is a one-variable change.

## Bumping the toolchain

Use the helper — it updates every pin and validates before you commit:

```sh
just bump-nightly 2026-06-01   # pin to a specific dated nightly
just bump-nightly              # pin to today's nightly (UTC)
```

It (1) rewrites the pin in every `rust-toolchain.toml` (root + per-crate, skipping
esp-channel files) and the workflow, (2) runs `just check`, and (3) leaves everything
in the working tree for review — it never commits. A failure now means a real
regression (a new clippy lint, genuine breakage), not a version-matching puzzle.

## Alternatives considered

Keeping the workspace on stable with per-crate nightly overrides (`lp-fw/fw-esp32c6/rust-toolchain.toml`,
etc.) would isolate nightly to firmware builds. This was rejected because:

- Three crates need nightly (`fw-esp32c6`, `fw-emu`, `emu-guest-test-app`)
- Justfile recipes would need `cd` into each crate directory for the local toolchain
  file to take effect
- The maintenance cost of split toolchains exceeds the risk of nightly regressions on
  host builds

This rejection stands for the rv32 crates. Future Xtensa firmware crates
(`fw-esp32s3`) are the one exception: they require the Espressif rustc fork
(`channel = "esp"`) because no upstream Xtensa target exists, and carry their own
per-crate `rust-toolchain.toml`. See
[ADR 2026-07-29-per-chip-fw-toolchains](adr/2026-07-29-per-chip-fw-toolchains.md).
