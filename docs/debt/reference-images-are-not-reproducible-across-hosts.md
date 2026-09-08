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
identical sha256s.

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

**Exit criteria** — one build environment for the reference images: a pinned
container (`Dockerfile.ci`, or a pinned image in the gated `emu-c6` job) that
both a desk and a runner can build in, verified by `--verify` producing the
same sha256 in both. That is an infrastructure decision with a real cost —
container build time on every emulator PR — and it belongs to Yona rather than
to a milestone; it was raised with M8's report. Until then the bands stay
bands, and that is a correct description of what is known, not a workaround.
