---
status: FIXED 2026-09-08 (this branch) — `links = "esp-hal"` on the fork
found: 2026-09-07      # L4 of the esp-emulator plan (PR #591), chasing a non-reproducible reference image
area: lp-fw/fw-esp32c6/build.rs, third_party/esp-hal
class: build-script-ordering
related: [lp2025/2026-09-06-1001-esp-emulator/l4-reproducible-reference-image.md]
---
# A cold `target/` links esp-hal's stock `rodata.x`, so build 1 ≠ build 2

**Symptom** — two builds of one commit produce two different images, and the
first one is the wrong one. In a fresh target directory:

| | sha256 | bytes | ROM-mapped sections |
|---|---|---|---|
| build 1 | `d0ac5bdc…` | 9,288,476 | `.flash.appdesc`, `.rodata_merge`, `.rodata`, `.rodata.wifi` |
| build 2 | `6ed5295b…` | 9,288,368 | `.rodata`, `.flash.appdesc` |

Build 2's layout is the one `build.rs` intends, the one the desk flashes, and
the one every committed C6 transcript was recorded against. Build 1's is
esp-hal's stock layout. **A CI tree is always cold**, so `firmware-size` and
every fresh checkout measured build 1 — an image nobody had ever run.

**Cause** — `fw-esp32c6/build.rs` patches a file that belongs to another
crate, on the hope that the other crate has already written it.

esp-hal's build script generates its linker scripts into its own `OUT_DIR`.
`build.rs` wants two of them changed: `rodata.x` merged into a single output
section (espflash turns the gaps between esp-hal's four sections into extra
ROM-mapped image segments, and the ESP32 bootloader asserts `rom_index < 2`),
and `eh_frame.x` flattened to a no-op. It found that directory by scanning
`target/<triple>/<profile>/build/` for an `esp-hal-*` entry with an `out/` in
it — and, finding none, **returned quietly**:

```rust
fn patch_file(path: &Path, contents: &str) {
    if !path.exists() {
        return;          // ← the defect
    }
```

Cargo runs build scripts concurrently unless something orders them, and
esp-hal declared no `links` key, so there was no edge between esp-hal's script
and this one. On a cold tree ours could win the race, scan an empty build
directory, patch nothing, and let the link take the stock script. Every later
build in that tree re-patched — which is why the bug is invisible on a warm
developer machine and unconditional on CI.

**Why it stayed silent.** It used to be loud. While `text.x` carried our
`__eh_frame` symbol, a pristine copy killed the link with
`undefined symbol: __eh_frame`, and that was the tripwire for the whole stale
set. The `text.x` patch went away with the unwind tier
(ADR `2026-08-02-rv32-firmwares-are-abort-tier`), and a pristine `rodata.x`
links perfectly well — it just produces an image whose extra ROM segments the
bootloader would reject. The symptom moved from the build to the device, and
then nobody flashed a first build, so it moved out of sight entirely.

**What it did not do** — this is not the `rom_index < 2` assert in the wild.
The stock script keeps its own merge section and its flash sections are
contiguous, so build 1 would very likely boot; it was never flashed. The
finding needs nothing stronger: it is a different image from the one the
transcripts came from, produced by the recipe that claims to reproduce them.

## The fix

Make the dependency real rather than hoped for. `third_party/esp-hal` — the
fork this repository already carries for its `#[ram]` diff — gains

```toml
links = "esp-hal"
```

and one line in its build script beside the `rustc-link-search` it already
emits:

```rust
println!("cargo::metadata=linker-scripts={}", out.display());
```

esp-hal links no native library; the key is there for the two things cargo
attaches to it. It **orders** a `links` package's build script before the
build scripts of everything that depends on it, and it **publishes** that
metadata to direct dependents as `DEP_ESP_HAL_LINKER_SCRIPTS`. So
`fw-esp32c6/build.rs` no longer guesses the directory and no longer races for
it: it is told, after the files exist.

Cargo also makes the dependent build script's fingerprint depend on esp-hal's,
which closes the second half of the race — a rerun of esp-hal's script (which
regenerates the scripts pristine) now re-runs ours *after* it, where the old
`rerun-if-changed` mtime watches could only catch it on the next build.

And the quiet return is gone. Both failure modes this script can see — no
`DEP_ESP_HAL_LINKER_SCRIPTS`, or a missing script inside the directory it
names — now abort the build with a message that says what to do. Skipping is
never better than stopping here, because what a skip produces is an image that
builds clean and dies in the bootloader.

**Second line of defence:** `just fw-esp32c6-rodata-layout-check` reads the
linked artefact and fails if it carries the stock layout — a `.rodata_merge`
or `.rodata.wifi` output section, or `.flash.appdesc` placed below `.rodata`.
It runs inside `fw-esp32c6-size-check`, which is what CI's `firmware-size` job
already calls, on the cold tree where this defect lived. Checking the recipe
was what failed here; this checks the output.

## Verified

Two builds in one cold target dir, `--features esp32c6`, this Mac:

| | build 1 | build 2 |
|---|---|---|
| before | `d0ac5bdc…` 9,288,476 B, stock layout | `6ed5295b…` 9,288,368 B, merged |
| after | `c339b900…` 9,288,728 B, **merged** | `c339b900…` **identical** |

The layout check passes on all three merged images and fails on the stock one,
by both of its signatures.

## Note for whoever drops the esp-hal fork

The `#[ram]` diff goes away when upstream takes it. **This one does not** —
nothing upstream provides a `links` key for esp-hal. If the fork is replaced
by a stock release, the key and the metadata line have to be re-applied on top
of it, or `fw-esp32c6/build.rs` has to stop needing them. It will refuse to
build rather than link the stock layout, and the panic names
`third_party/esp-hal/README-LP.md`, so the mistake cannot be silent — but it
will stop the build. See that README's "The second diff".

## What this does not change

`scripts/emu/build-reference-image.sh` keeps its `.rodata_merge` heal. That
recipe builds a **pinned historical commit** (`d6cfaa205`), which predates
this fix and still has the race; the workaround is load-bearing for exactly
that reason, and its comment now says so.
