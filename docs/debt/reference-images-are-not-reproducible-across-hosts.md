---
status: carried
since: 2026-09-07
logged: 2026-09-08
area: scripts/emu/build-reference-image.sh + CI's emu-c6 job
related:
  - docs/adr/2026-09-06-esp-soc-emulator-architecture.md
  - docs/reports/2026-09-08-esp32c6-emulator-walk.md
  - docs/debt/local-gate-misses-what-ci-checks.md
---
# A reference firmware image is reproducible on one host and not between two

**Shape** — the ESP32-C6 emulator's gates compare a run against a transcript
recorded from a named image, so "the same image bytes" is the premise
underneath every one of them. `scripts/emu/build-reference-image.sh` now makes
that true **within** a host: L4 (PR #591) found and fixed three build
non-determinisms — `esp_app_desc!()`'s wall-clock timestamp
(`SOURCE_DATE_EPOCH=0`), absolute paths in `.debug_str` including the
host-triple sysroot (`--remap-path-prefix`), and a cold-tree race where
`fw-esp32c6/build.rs` runs before esp-hal's out dir exists and links the stock
`rodata.x` — and two builds in two directories on one machine now produce
identical sha256s. (The third of those was a rebuild-once workaround inside
the reference-image script. PR #601 fixed the **product** build underneath it,
by giving the vendored esp-hal a `links` key so the two build scripts have a
real ordering edge: build 1 of a cold tree is now the right image, which
matters far beyond this entry — `firmware-size` and every fresh checkout had
been measuring an image nobody had ever flashed.)

**Between** hosts they still differ by ~4.7 KB. Everything a build can be told
is pinned; the residual is the **rustc binary's own host build** — the same
nightly hash, a different `.text` — so the code lands at different addresses
and the image is a different size. Three CI runs of one pinned commit gave
9,108,248 B against this host's 9,103,476 B, with `Rmt::new`'s `sys_conf`
write at `0x4207713e` rather than `0x42076e6c`, and 47,679,820 against
47,681,337 instructions to the same deadline.

The consequence is a split in what the gates may assert. Every register access
the guest makes is identical in value on both hosts, and every **memory-class**
figure is byte-equal — so those stay exact. Figures that depend on *where the
code is* do not survive: the `[stack]` high-water is the deepest point an
interrupt ever landed on the main task, and a tick that lands on a different
instruction of a differently laid-out image has a different deepest point
(11,432 B here, 11,560 B on the runner). Those are asserted as **bands with a
digest** (DD45/DD46), never as numbers.

**Carrying cost** — a permanent asterisk on the plan's strongest claim. Every
new gate has to be classified before it is written (memory class: exact;
timing or stack class: band), every band's width has to be justified from a
measured spread rather than fitted, and every report that quotes a stack or
instruction figure has to say which host produced it. The cost lands on
whoever writes the next gate, repeatedly, and the failure mode when they get
it wrong is a red CI run on a correct change.

**Workarounds**

- `scripts/emu/build-reference-image.sh --verify` builds twice and prints
  `rustc -vV` plus a per-section digest table, which is how a host proves its
  own reproducibility and how two hosts' images are compared.
- Classify a new figure before writing its assertion. Memory class exact,
  everything positional a band with the run digest printed on failure.
- Quote both hosts' tables when a report states a positional figure.

**Incident log**

- 2026-09-07 (M5 P1) — three CI runs of one commit gave three ELF shas; the
  memfs `[stack]` gate went to a documented band. DD45.
- 2026-09-07 (M4) — the same for the flash-backed image; the rule was made
  uniform rather than per-image. DD46.
- 2026-09-07 (L4, PR #591) — within-host reproducibility fixed; the
  cross-host residual identified as the toolchain's own build. DD49.
- 2026-09-08 (M7, PR #596) — the drift showed up in *layout*, not only size:
  this tree's ELF at the transcript's commit links `.rodata` 0x20 higher, so
  espflash splits the app into six segments where silicon's had five. Any gate
  on image layout rather than memory content needs a re-record or a documented
  split.
- 2026-09-24 (#797, CI speed) — emu-c6 now caches the pinned reference
  builds' outputs, keyed on runner OS/arch + recipe + toolchain so a hit only
  ever returns bytes this runner image built; `--verify` (both C6 and the
  classic) runs on PRs only when the recipe, `rust-toolchain.toml` or the
  classic's firmware changes, and always on main. A within-host
  reproducibility break from anything else is now caught post-merge.
- 2026-10-06 (#987, emulator seams P10) — a **memory-class** figure moved
  between hosts, which this entry says does not happen. `just test-emu-c6` on
  the desk built the pinned `d6cfaa205-esp32c6+server+radio` reference image
  itself (sha256 `cde3680c…`, CI's is `fadf7653…`), and `emu_serve_walk`'s
  exact heap gate read `220468 B free / 105068 B used` against the pinned
  `220532 B free / 105004 B used`, 64 B apart. The same test, the same
  emulator, against CI's image (`just fetch-ci-images 987 esp32c6`,
  `LP_CI_IMAGES=…`) passes. Workaround that held: run the C6 suite against
  CI's images rather than desk-built reference images. Not investigated
  further — where a host-built image spends 64 B more heap is the open
  question.
- 2026-10-07 (#1019, Wi-Fi relay P9) — the same 64 B on the desk: the first
  `emu_serve_walk` run (`LP_EMU_BUILD_FW=1`, which built the pinned
  reference image) read `220468 B free / 105068 B used`; an immediate re-run
  of the one test passed (5.7 s, the reference build cached). Not
  investigated; this branch's own firmware is not in that test.
- 2026-10-21 (#1237, S3 radio M2) — the same 64 B a third time, the same
  shape: inside `just test-emu-c6` (`LP_EMU_BUILD_FW=1`, the desk building
  the pinned reference image under a load average near 100) the first
  `emu_serve_walk` read `220468 B free / 105068 B used`; the one test re-run
  alone passed (5.95 s). The failure ends `test-emu-serve` at its first line,
  so the rest of `test-emu-serve` and all of `test-emu-c6-cli` had not run
  and needed running on their own. CI's Emulator C6 job passed on the same
  head. The branch's own firmware is not in that test.
- 2026-10-21 (#1249, S3 radio M5) — the fourth desk sighting, byte for byte:
  `just test-emu-c6`'s first `emu_serve_walk` run, which built the pinned
  reference image, read `220468 B free / 105068 B used`; a re-run of
  `just test-emu-serve` passed whole (the walk in 5.8 s, cached). Four of
  four now: the 64 B appear on the run that *builds* the reference image
  and are gone on the run that reuses it — a lead for whoever takes the open
  question, not investigated here.
- 2026-10-22 (#1258, S3 updates M7 P8) — the fifth, byte for byte and the
  same shape: `just test-emu-c6`'s first `emu_serve_walk` (the run that built
  the reference image, load average ~155) read `220468 B free / 105068 B
  used` and ended `test-emu-c6-boot` there; `just test-emu-serve` re-run
  passed whole, and `test-emu-c6-cli`, which the failure had skipped, was
  run on its own. Five of five on the building run.
- 2026-10-22 (#1273, S3 radio M5's second half) — the sixth, byte for byte:
  `just test-emu-c6`'s first `emu_serve_walk` (building the reference image,
  load average ~60–110 with an S3 walk running beside it) read `220468 B free /
  105068 B used`; 592 of the suite's 593 tests passed. `test-emu-serve`,
  re-run on its own, passed whole (the walk's figure exact, the reference
  image reused); `test-emu-c6-cli`, which the failure had skipped, was run
  on its own. The branch's C6 image
  is byte-identical to main's but for its build stamp, so this test's figure
  cannot be the branch's. Six of six on the building run.
- 2026-10-25 (#1303, S3 updates M7 P11 checks) — the seventh, byte for byte:
  `just test-emu-c6`'s first `emu_serve_walk` (building the reference image)
  read `220468 B free / 105068 B used` and ended `test-emu-c6-boot` there.
  `test-emu-c6-cli`, which it had skipped, passed on its own;
  `test-emu-serve`, re-run, passed whole (load average ~30–45). The PR's C6
  jobs were path-skipped, so this local run was the merge's evidence (the
  diff touched `fw-esp32s3` only). Seven of seven on the building run.
- 2026-10-27 (#1319, mapping rebuild M2b) — the eighth, byte for byte:
  `just test-emu-serve`'s first `emu_serve_walk` (building the pinned
  `13b04dd0b` reference lp-cli and image) read `220468 B free / 105068 B
  used`; the one test re-run with the build cached passed (6.1 s), and the
  LAN cells after it passed. This branch's firmware is not in that test.
  Eight of eight on the building run.

**Exit criteria** — one build environment for the reference images: a pinned
container (`Dockerfile.ci`, or a pinned image in the gated `emu-c6` job) that
both a desk and a runner can build in, verified by `--verify` producing the
same sha256 in both. That is an infrastructure decision with a real cost —
container build time on every emulator PR — and it belongs to Yona rather than
to a milestone; it was raised with M8's report. Until then the bands stay
bands, and that is a correct description of what is known, not a workaround.
