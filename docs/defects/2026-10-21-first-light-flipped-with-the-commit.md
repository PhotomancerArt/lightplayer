---
status: fixed
found: 2026-10-21      # how: PR #1227 (best-link M3) could not go green on `C6 first light (x64)`, a gate its diff does not touch
fixed: 2026-10-21
area: tools/lp-app-version (`figure_build`, the figure build's stamps) and the ESP firmware build scripts' `LP_BUILD_COMMIT`
class: nondeterministic-capture
related:
  - docs/first-light-ratchet.md
  - docs/debt/heap-budget-record-churns-on-routine-changes.md
---
# The first-light ratchet read 386 or 391 ms by the commit alone

**Symptom** — `CI / C6 first light (x64)` read the gated `basic+net` case as
386 ms on some commits and 391 ms on others, on PRs whose diff touched no
firmware (`c6_image: false`). The record is 385 ms with +5 ms of tolerance,
so a 391 commit was red ("apply CI's re-record"), and after the re-record
the next commit read 386 and was red the other way ("dropped 391 → 386").
PR #1227 was blocked on it. The phase that moved was the firmware's own
`lit` (193 vs 188 ms); the frame on the pad started at 384.0 ms in both.

**Root cause** — a figure build (`LP_FIGURE_BUILD=1`) pinned its version
(`0000000`) and its dirty flag, but `LP_BUILD_COMMIT` stayed
`git rev-parse --short=12 HEAD`, and on a PR that is GitHub's merge commit,
new on every push. The commit is in the C6's `merged.bin` five times (the build
id in the core and in the engine header, the boot banner's `commit=`, the
hello's `"commit"`, and once more in the engine). The linker merges the
core's `.rodata` strings by content, so a different commit string landed in a
different place and reordered the strings around it. CI's two images from
PR #1227 (runs 39577661191 on head f66f66acc and 39603185842 on head
0d76c8810, firmware source trees identical by `ci-images` digest
`4a59d2ae…`) differ only that way: the loader is byte-identical; in `p2.elf`
the same 53,915 symbols with the same sizes, no function at a different
address, and 906 `.Lanon.*` read-only constants at different addresses, so
the core's `.rodata` pages differ heavily (37,571 bytes in
`0x10000–0x2ffff`) and the instructions that address those constants differ
in scattered bytes through the core and the engine (39,988 bytes in
`merged.bin` in all). `lp-emu:esp32c6:t4` charges for where code and data
land in flash and cache, so the same firmware read 5 ms apart.

**Fix** — `lp_app_version::commit()` stamps a figure build with
`FIGURE_COMMIT` (`000000000000`, the length of a real stamp, so nothing
that depends on the length moves), and the C6, S3 and classic build scripts
take their `LP_BUILD_COMMIT` from it. A product or release build is never a
figure build: its commit, build id, boot records and ota-manifest are
unchanged.

Proven on CI after the merge: the PR's run (merge commit `fb23401a2`) and
main's (`94d83eea6`), same sources, read `basic+net` at 386.950 and
386.797 ms and `c40+net` at 402.406 and 402.364 ms, identical to the
microsecond. Their images still differ in 37 bytes: the ESP app descriptor's
build time (`esp-bootloader-esp-idf` stamps the wall clock unless
`SOURCE_DATE_EPOCH` is set), a fixed-width field in `.flash.appdesc`, and the
image checksum and digest that cover it. That moves nothing, so it is left;
pin it too (`SOURCE_DATE_EPOCH` in a figure build) if a figure ever needs
byte-identical images rather than an identical layout.

**Lesson** — the version and the dirty flag were pinned because their
*length* reached a heap figure; the commit was left real because its length
never varies. A timing figure depends on *placement*, which any byte of
content can move. A figure build must hold every checkout-derived byte
still, not only the ones whose length matters.
