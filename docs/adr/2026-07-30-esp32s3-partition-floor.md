# ESP32-S3 targets an 8 MB flash floor

- Status: accepted
- Date: 2026-07-30
- Context: M3 of `2026-07-30-s3-app-layer-with-jit` (app layer on the S3)

## Context

`fw-esp32s3`'s `partitions.csv` was a byte-for-byte copy of the C6's: 3 MB
`factory` + 960 KB `lpfs`, exactly 4 MB. That was a deliberate choice when the
crate was a boot skeleton — matching the C6 meant the storage layer's offsets
ported across unchanged, and 4 MB was treated as the floor any ESP32-S3 board
would clear.

Bringing the app layer up forces the question, because the copy is probably too
small:

- The C6's full image is **~2,864,672 B of a 3 MB partition** — about 281 KB of
  headroom, on RISC-V *with* compressed instructions.
- Xtensa LX7 has no equivalent compressed encoding, so the same application is
  expected to be meaningfully larger on the S3.
- The S3 is also the chip slated to carry features the other builds do not.

So a 3 MB app partition on the S3 is likely to overflow on arrival, and the
options are to fight for space with feature gates or to change the table.

## Decision

**The S3 targets an 8 MB flash floor.**

```
nvs,      data,  nvs,     0x9000,   0x6000,
phy_init, data,  phy,     0xf000,   0x1000,
factory,  app,   factory, 0x10000,  0x600000,   # 6 MB
lpfs,     data,  spiffs,  0x610000, 0x180000,   # 1.5 MB
```

Ends at 0x790000 of 0x800000 — 448 KB of slack. The C6's table is unchanged.

This **narrows supported hardware** from "any ESP32-S3" to modules with at
least 8 MB of flash. An N4 (4 MB) board cannot flash this image. That exclusion
is the substance of the decision, not a side effect.

## Alternatives considered

The choice was made against sourcing data rather than assumption, after
checking how the ESP32-S3 module landscape actually looks in 2026.

**16 MB (rejected).** The desk board is a 16 MB N16R8, and it was the initial
answer. Rejected on two findings: N8R8 and N16R8 are *both* among the two
most-sourced WROOM-1 variants, so 16 MB is not a safe default; and N16R8
carries the longest authorized-channel lead times (6–12 weeks via
Mouser/DigiKey versus 4–8 for N8R8) precisely because demand is highest. A
16 MB table would strand N8 boards for headroom with no identified use.

**Keep 4 MB (rejected).** Defensible on compatibility — N4 is not
discontinued and remains stocked — but it loses on measurement. A 3 MB app
partition that the image is expected to exceed converts every subsequent
milestone into a flash fight, which is exactly the pressure the C6 already
lives under and the S3 need not.

**Per-board table selection (deferred).** Build-time selection among 4/8/16 MB
tables is probably where this ends up, and it is the only option that encodes
no assumption. It was deferred because it is real machinery and does not belong
inside a bring-up milestone. Revisit when a second S3 board with a different
part actually exists.

## Consequences

- **The S3's image size becomes a trend, not a budget gate.**
  `just fw-esp32s3-size-check` now measures against 6 MB and exists so Xtensa
  code density stays tracked — a number that still governs the C6 and the
  classic ESP32, both of which are genuinely constrained. The margin should not
  be tightened to manufacture pressure.
- **The roadmap's central question is partly retired.**
  `2026-07-30-s3-app-layer-with-jit` asked "does the app layer fit in 3 MB of
  Xtensa flash?" On the S3 it no longer has to. The second half — "what does
  each feature cost?" — stands, and M2's node gates keep their value for the
  C6 and for keeping the bring-up build minimal.
- **A board assumption now lives in `partitions.csv`.** Its failure mode is a
  flash step that fails on an N4 with no obvious explanation, so it is stated
  in the file's own header comment and in the crate README rather than left
  implicit.
- **The floor must be declared twice, and the second one is easy to miss.**
  espflash writes a flash-size field into the image header and **defaults it to
  4 MB**; the bootloader validates the partition table against that header
  rather than against the physical chip. So `--flash-size 8mb` is as mandatory
  as `--partition-table`. Discovered the hard way while landing this ADR: the
  first flash of the new table boot-looped on a board with 16 MB physically
  soldered on, with

  ```
  I (45) boot.esp32s3: SPI Flash Size : 4MB
  E (56) flash_parts: partition 2 invalid - offset 0x10000 size 0x600000
         exceeds flash chip size 0x400000
  E (66) boot: Failed to verify partition table
  ```

  It is now carried in the justfile as `s3_flash_size`, used by both S3 flash
  recipes, and duplicated in `lp-fw/fw-esp32s3/.cargo/config.toml`'s runner
  (which cannot read a justfile variable). All three must move together, and
  all three say so.
- The pre-existing espflash trap gets sharper: omitting `--partition-table`
  makes espflash silently substitute a default with a 1 MB factory partition.
  Our table has diverged further from any default, so that substitution now
  fails later and more confusingly.
- `docs/adr/2026-07-28-esp32c6-flash-budget.md` is unaffected and still governs
  the C6.

## References

- `lp-fw/fw-esp32s3/partitions.csv`, `lp-fw/fw-esp32s3/README.md` (Partitions)
- `docs/adr/2026-07-28-esp32c6-flash-budget.md` — the C6's budget, unchanged
- `docs/adr/2026-07-29-per-chip-fw-toolchains.md` — the per-chip posture this
  extends from toolchains to flash layout
- Sourcing data and the full landscape survey:
  `~/.photomancer/planning/lp2025/2026-07-30-s3-app-layer-with-jit/03-app-layer-on-s3/notes.md`
