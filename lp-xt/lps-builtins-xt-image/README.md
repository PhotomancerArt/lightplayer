# lps-builtins-xt-image

Embeds the **Xtensa builtins image** at build time and hands it to host-side
consumers as `&'static [u8]`.

The image itself is built by `lp-xt/lps-builtins-xt-app` — a DEVICE-target crate
in the `lp-xt/fixtures` esp workspace — and lands at
`lp-xt/fixtures/elf/lps-builtins-xt-app.elf`, which is **not** checked in:

```bash
scripts/build-builtins-xt.sh    # needs the esp toolchain (espup)
```

## What it is for

`lpvm-native`'s `rt_emu` runs compiled shader code on the host. Shader code calls
builtins (`sin`, `cos`, `sqrt`, …), so it needs a base image carrying those
builtins as real guest code at the addresses `lp-xt-emu` models. The engine loads
this image, places the shader's functions after its `.text`, and patches call
relocations against the merged symbol map.

This is the Xtensa counterpart of what `lpvm-cranelift/build.rs` does for the
rv32 image (`lps-builtins-emu-app`). Both images carry the same 24 `__lps_*`
builtins, from the same `lps-builtins` source.

## Not built is a normal state

`image()` returns an **empty slice** when the ELF is absent, and
`is_available()` reports it. The workspace must build and test on a machine with
no esp toolchain, so consumers skip the Xtensa host path with a loud note naming
`BUILD_COMMAND` rather than failing.

## Why a separate crate

- `lp-shader/*` crates are **sans-IO** (`docs/adr/2026-07-06-sans-io-core.md`), so
  the consumer cannot read the ELF from a path at runtime. Embedding at build
  time is the compliant answer.
- The consumer, `lpvm-native`, is also compiled for device firmware. A build
  script there would run on every firmware build to do nothing.

## The race this build script guards against

`scripts/build-builtins-xt.sh` rewrites the ELF while other builds may be reading
it, and this script declares `rerun-if-changed` on exactly that path — so the
rewrite is what wakes it. Treating that window as "not built" would embed an
empty slice and surface minutes later as every Xtensa test skipping at once. The
copy therefore verifies ELF magic and a stable size, retrying briefly. Same
hazard and same fix as the rv32 image; see
`docs/defects/2026-07-29-builtins-elf-uplift-race.md`.

**"Every Xtensa test skipped or failed at 0.00s" means a missing or half-written
image, not a codegen bug.**
