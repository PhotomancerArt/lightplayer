# CI's firmware images, on a desk: the chip suites with no firmware build

Reproducing a chip-emulator failure used to start with a firmware build: an
Xtensa or RISC-V cross-build (5–15 minutes on a loaded desk) and tens of GB of
`target/` per worktree. CI has already built exactly those images. So the
three emulator jobs upload them, and one variable points the local recipes at
them:

```bash
just fetch-ci-images                  # the newest GREEN main run, all three chips
just fetch-ci-images 830              # a PR's newest run with images (red runs too)
just fetch-ci-images 830 esp32s3      # one chip
just fetch-ci-images <sha>|<run-id>   # a commit, or one run
# … prints: export LP_CI_IMAGES=/…/target/ci-images/<commit12>

export LP_CI_IMAGES=/…/target/ci-images/<commit12>
just test-emu-esp32s3-boot            # the S3 suite, CI's ELF + merged chip
just test-emu-esp32v3-boot            # the classic's, all five images
just test-emu-c6-boot                 # the C6's, tree + pinned reference images
just heap-budget-check-chips-s3       # a chip heap ratchet on CI's image
just bless-chips esp32v3              # re-record figures against CI's image
just ci-images-status                 # what is fetched, and does it match HEAD
```

Unset `LP_CI_IMAGES` and every recipe builds its own images exactly as before.

## What is uploaded

| job | artifact | what |
|---|---|---|
| `Emulator C6 (x64)` | `ci-images-esp32c6` | every tree image the suite built (`tree/<SLUG>/fw-esp32c6`: shipped, no-radio, memfs, the RMT harnesses) and every pinned reference image it booted (`emu-ref/<commit>-<slug>/`: ELF, `merged.bin`, `SHA256SUMS`, `PROVENANCE`) |
| `Emulator ESP32v3 (x64)` | `ci-images-esp32v3` | the shipped, `rmt-chase` and `frame-dump` ELFs, the merged chip, and the pinned `75486b114` reference ELF + merged chip + its partition table |
| `Emulator ESP32-S3 (x64)` | `ci-images-esp32s3` | the shipped ELF and the merged 8 MiB chip |

Each carries a `manifest.json`: the commit CI built (the PR's merge commit, and
the PR head beside it), the run, the runner, espflash's version, per file its
sha256, size and features/profile, the environment variables that name each
file, and **the git tree id of every firmware source path** (below). Retention
is 7 days; the artifacts are uploaded even when the job's tests failed —
a red run's images are the ones most worth having.

The v3 and S3 boot recipes write `target/lp-emu-esp32{v3,s3}/images.env` (the
variables they set) and the packer takes exactly those files; the C6's are
`lp_emu_esp32c6::test_support`'s own keyed copies. The heap ratchets' shipped
image is the same bytes the boot suite reads (same features, same profile), so
nothing is uploaded from `Heap budget (esp32c6 chip)` or the `Firmware build`
jobs, and `Emulator ESP32v3 reference` uploads nothing either: its only
firmware build is the reproducibility claim itself, which is about the host
that runs it (below).

## The refusal: images from other sources are not your images

`fetch`, and every recipe at use time, compare the manifest's source trees with
this checkout's `HEAD` **and** its working tree, over:

`lp-fw lp-core lp-base lp-shader lp-gfx lp-riscv lp-xt lp-app/lpa-server
third_party Cargo.toml Cargo.lock rust-toolchain.toml`

— the link closure of the three chip firmwares, the lockfile and the toolchain
pin. Any difference (a commit, or an uncommitted or untracked file there) is
refused by name:

```text
ci-images: esp32s3: CI's images were built from different firmware sources than this checkout (images: …, run …; HEAD: …).
  lp-fw: images built from 1a2b…, HEAD has 3c4d…
  A suite run against these images would test someone else's firmware. …
```

`LP_CI_IMAGES_FORCE=1` (or `fetch --force`) accepts it anyway and says so on
every use. `lp-emu/` is deliberately *not* a source path — the emulator and its
tests are what you change while testing against CI's images. Nor is the
justfile or `scripts/`: the build recipes' profiles are fixed, and a change to
how an image is made is a reason to let CI rebuild it. Pinned reference images
(`emu-ref/<commit>-<slug>`) are keyed by their commit and never stale.

Because a PR run builds its **merge commit**, a branch that is behind `main`
on those paths is refused against its own PR's images. Merge `origin/main`
(which is what CI tested), or force it knowing the difference.

Every file's sha256 is re-checked on every use, so an edited or truncated
image fails rather than being booted.

## What still builds locally

- **Host binaries.** The emulators and the test binaries are host builds and
  compile as usual. `emu_serve_walk`'s pinned proto-20 `lp-cli` is a host
  binary too; CI's is Linux, so it still builds locally unless `LP_EMU_REF_CLI`
  names one.
- **`test-emu-esp32v3-reference`'s reference image.** Its `--verify` claim —
  two cold builds, one sha256 — is about the host that runs it; under
  `LP_CI_IMAGES` the recipe skips it and says so. `test-emu-xt-jit-image`
  (the translator's identity cell) boots CI's shipped v3 image instead.
- **ESP-NOW images** (`LP_EMU_C6_ESPNOW_ELF`): CI does not build them either.

## Figures and blessing

`just bless-chips` composes: with `LP_CI_IMAGES` set every chip step — the heap
record through `heap-budget-baseline-chips*` and the figure record through the
chip's boot suite — reads CI's images, so a bless takes seconds of emulation
instead of a firmware build. `[positional]` figures stay CI-only: a desk bless
still does not write them (docs/chip-figures.md, "Positional figures"), even
though the bytes are CI's. With CI's images a positional failure on the desk
does read CI's value, which is the value to copy.

When CI has already measured the move, `just apply-ci-figures` (the
`figures-patch-*` artifacts) is cheaper still — nothing runs at all. Fetch the
images when you need to *run* something: to debug a failure, try an emulator
change against a known image, or bless after an emulator-side edit.

## The C6's image directory

`lp_emu_esp32c6::test_support` reads `LP_EMU_C6_IMAGE_DIR` (set for you by
`scripts/ci/ci-images.py with esp32c6`): `tree/<SLUG>/fw-esp32c6` for a
feature-set image, `emu-ref/<commit>-<slug>/{fw-esp32c6,merged.bin}` for a
reference image. With it set, nothing is built and a missing image **panics**
rather than skipping — a skip there would be a green run against nothing. The
explicit per-image variables (`LP_EMU_C6_ELF_<SLUG>`, `LP_EMU_C6_REF_<SLUG>`)
still win.

## Pieces

- `scripts/ci/ci-images.py` — `pack` (CI), `fetch`, `env`/`with` (the recipes'
  hook, a no-op when `LP_CI_IMAGES` is unset), `status`.
- `.github/workflows/pre-merge.yml` — the `Pack CI images` / `Upload CI images`
  steps in `emu-c6`, `emu-esp32v3`, `emu-esp32s3`.
- `justfile` — `fetch-ci-images`, `ci-images-status`, and the `LP_CI_IMAGES`
  branches in `test-emu-{c6,esp32v3,esp32s3}-boot`, `test-emu-c6-cli`,
  `test-emu-serve`, `heap-budget-{check,baseline}-chips*`,
  `test-emu-esp32v3-reference` and `test-emu-xt-jit-image`.
