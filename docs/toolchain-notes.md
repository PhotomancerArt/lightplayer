# Toolchain Notes

The workspace uses **nightly Rust** (`rust-toolchain.toml`). This is required by the
bare-metal firmware targets; host crates compile fine on stable but share the workspace
toolchain for simplicity.

## Why nightly

Three features used by `fw-esp32` and `fw-emu` are unstable:

1. **`#![feature(alloc_error_handler)]`** — Custom OOM handler that panics normally
   (the default handler uses `nounwind` panic, which `catch_unwind` can't intercept).
   Used in `fw-esp32/src/main.rs`.

2. **`-Zbuild-std`** — Rebuilds `core` and `alloc` from source with `panic = "unwind"`.
   The pre-built sysroot for `riscv32imac-unknown-none-elf` uses `panic = "abort"`;
   mixing strategies causes a linker error. Configured in `fw-esp32/.cargo/config.toml`.

3. **`#[lang = eh_personality]`** — Provided by the `unwinding` crate to implement the
   Itanium EH personality routine in `no_std`. This is a lang item, which is unstable.

All three are needed for OOM recovery via stack unwinding on ESP32 and the RISC-V
emulator. See `docs/reports/2026-03-13-esp32-unwinding-implementation.md` for details.

## Why the nightly is pinned (and why it's coupled to `unwinding`)

The toolchain is pinned to a dated nightly (e.g. `nightly-2026-04-27`), **not** a
rolling `nightly`. The pin lives in two places that must stay in sync:

- `rust-toolchain.toml` — drives local dev and any in-repo `cargo`/`rustc` call.
- `.github/workflows/pre-merge.yml` — the `dtolnay/rust-toolchain` step (CI checks
  out into a subdirectory, so the action can't auto-read the toml; the date is
  passed explicitly).

The reason it's pinned rather than rolling: this is a `-Zbuild-std` project, and the
[`unwinding`](https://crates.io/crates/unwinding) crate (our `eh_personality`
provider) is bound to the nightly `core::intrinsics::catch_unwind` ABI. That
intrinsic changed its return type from an integer to `bool`:

- `unwinding` **0.2.8** expects the integer form (`catch_unwind(...) == 0`).
- `unwinding` **0.2.9** expects the `bool` form (`if catch_unwind(...) { ... }`).

So the `unwinding` version and the nightly are a matched pair — there is no single
`unwinding` that builds on both an old and a new nightly. With an unpinned `nightly`,
CI silently drifts onto a newer toolchain than local dev and the build-std compile
breaks (`E0308: expected bool, found integer`). Pinning keeps CI reproducible and in
lockstep with local.

## Bumping the toolchain

Use the helper — it updates both pins, moves `unwinding` only if the new nightly
requires it, and validates before you commit:

```sh
just bump-nightly 2026-06-01   # pin to a specific dated nightly
just bump-nightly              # pin to today's nightly (UTC)
```

It (1) rewrites the pin in `rust-toolchain.toml` and the workflow, (2) runs
`just check` with the current `unwinding` (this compiles `unwinding` under build-std
via `clippy-rv32`, and also surfaces any new clippy lints from the newer nightly),
(3) only if that fails, advances `unwinding` to the latest `0.2.x` and re-checks, and
(4) leaves everything in the working tree for review — it never commits. If the new
nightly can't be made to build (e.g. `unwinding` needs a new *major*, or a new lint
fires), it reports what to try and reverts only the speculative `unwinding` bump.

## Alternatives considered

Keeping the workspace on stable with per-crate nightly overrides (`lp-fw/fw-esp32/rust-toolchain.toml`,
etc.) would isolate nightly to firmware builds. This was rejected because:

- Three crates need nightly (`fw-esp32`, `fw-emu`, `emu-guest-test-app`)
- Justfile recipes would need `cd` into each crate directory for the local toolchain
  file to take effect
- The maintenance cost of split toolchains exceeds the risk of nightly regressions on
  host builds
