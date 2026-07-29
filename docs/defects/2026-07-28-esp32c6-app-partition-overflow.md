---
status: open
found: 2026-07-28      # how: live-debugging (studio-dev startup failed)
area: lp-fw/fw-esp32 (partitions.csv), justfile studio-firmware-package-esp32c6
class: budget-exhaustion
related:
  - docs/debt/legacy-mapping-variants.md
---
# ESP32-C6 app image no longer fits the 3 MB app partition

**Symptom** — `just studio-dev` fails at its
`studio-firmware-package-esp32c6` prerequisite:

```
espflash::image_too_big
  Supplied ELF image of 3178272B is too big, and doesn't fit configured
  app partition of 3145728B
```

The dev server never starts. Earlier the same day (pre-merge tree) the
package step reported `App/part. size: 3,119,168/3,145,728 bytes,
99.16%` — the budget was already one merge away from the wall, and the
merge of ~126 commits from `main` (plus a few KB of new fixture-engine
code) tipped it by ~32 KB.

**Root cause** — flash layout is 4 MB total: 3 MB `factory` app +
960 KB `lpfs` (device project storage). The app binary has grown to the
partition boundary with no headroom and **no CI guard**: the pre-merge
CI builds `fw-esp32` but never runs the espflash image-fits-partition
packaging step, so `main` can (and did) cross the line silently — the
failure only surfaces on the next `just studio-dev`.

**Fix** — partial (same day): the legacy mapping-variant retirement
(`SvgPath` + `RingArray` deletion, debt register entry) freed ~42 KB —
3,178,272 → 3,136,320 B, the image fits again at **99.70%**. That is
~9 KB of headroom: the next feature lands on the wall again. Still
open, in decision order: (a) grow the app partition at `lpfs`'s expense
(device-layout change; reflash + stored-project loss); (b) a real size
dive (the −431 KB tagging/VecMap round is precedent); (c) the ESP32-S3
pivot (16 MB flash) may moot the budget entirely. The `studio-dev-nofw`
launch-entry workaround is no longer needed but kept documented.

**Regression coverage** — none: the missing coverage IS the finding.
The packaging (or at least an image-size assertion against
`partitions.csv`) belongs in `just build-ci` so the wall is hit in CI,
not at the next dev-server start.

**Lesson** — a budget that only the dev-loop enforces will be exceeded
by whoever merges last, and they will be the least responsible for the
growth. Budgets need a CI assertion at the same threshold as the tools
that consume them — and ideally a headroom warning (e.g. fail at 100%,
warn at 95%) so the wall is visible while there is still room to plan.
