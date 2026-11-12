---
status: fixed
found: 2026-10-21      # test (lp-emu:esp32s3:t1, --flash-cut, lp-nor-sim calibrated)
fixed: 2026-10-21      # M7 P9: lp-emu-esp32s3's power cycle restores the flash's supply
area: lp-emu-esp32s3 `then=power-cycle` (first seen as fw-esp32s3 lpfs, littlefs)
class: fidelity
related:
  - 2026-10-08-littlefs-rust-relocation-cut-leaves-lpfs-unmountable
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M7 P5 found it; P9 found the cause)
---
# A power cut during the S3's first lpfs format leaves a filesystem that mounts and says it is full

**Symptom** — On the emulated S3, a power cut on the 6th in-range flash
command (`--flash-cut 5:calibrated:1,then=power-cycle`: a 32-byte program at
`0x611020`, during the first boot's format of the blank `lpfs` in a fresh
merged chip) is followed by a second boot that prints `[INIT] flash
filesystem mounted` — not a re-format — and serves; but the upload's first
write is refused:

```
M!{"id":3,"msg":{"filesystem":{"write":{"path":"/projects/Basic/clock.json",
  "error":"Filesystem error: mkdir projects: no space left on device"}}}}
```

The board stays reachable (hello, link and server all up), so no cable is
needed to reach it; what it cannot do is hold a project until `lpfs` is
formatted again.

**Reproduce** (`lp-emu:esp32s3:t1`, the shipped image built at `eb03e6d31`'s
tree plus M7 P5):

```bash
cp merged.bin s3.bin   # espflash save-image --chip esp32s3 --merge … of the shipped image
lp-cli emu run --chip esp32s3 --host-link --rom-up-flash s3.bin \
  --flash-cut 5:calibrated:1,then=power-cycle --upload projects/test/basic --timeout 6s
```

**Cause** — the emulator, not littlefs and not the firmware. The S3
machine's `power_cycle` restored its power-on snapshot but never the flash
image's power (`FlashImage::restore_power`, which the C6's `restart` calls on
a power cycle). The chip stayed latched off after the cut and dropped every
program and erase the second boot sent, while reads still answered. littlefs
mounted what the first boot had left and every write it then tried failed
to land, which it reported as no space. On the tree store (M7 P9) the same
run re-formatted, verified nothing, retired every sector and gave up with
`format failed: NoSpace`. The tell was the chip itself: after the run its
`lpfs` was byte for byte what the cut left, and `FLASH-OPS` counted no
in-range op after the cycle. The same chip, written back with `then=stop` and
booted in a fresh process, formatted at once, as the C6 does over the
identical cut.

**Fix** — `lp-emu-esp32s3`'s `Machine::power_cycle` calls `restore_power()`
(M7 P9). `tests/flash_cut.rs`'s power-cycle cut now asserts the second
boot's format lands; it fails with the call removed.

**Evidence** (`lp-emu:esp32s3:t1+flash-cut`, the P9 split image with the
tree store, `calibrated` seed 1): a cut at every in-range op 0–15 of the
first boot (its whole format, ops 0–13, and the first two after it), each
`then=power-cycle` followed by `--upload projects/test/basic`. Every second
boot formats (ops 0–13) or mounts (14, 15) and commits the upload, and a
third boot mounts and auto-loads `/projects/Basic`. A cut inside a second
push (`projects/test/basic2`, ops 30 and 60) leaves the store as it was
(`root 3`, `/projects/Basic` loads).

**What is not known** — whether silicon tears these programs the way
`lp-nor-sim`'s `calibrated` model does (calibrated on the C6's part); the
S3's desk sitting (P11) is where a real cut is seen. littlefs's behaviour
under a real cut on this chip is moot: it is no longer on the S3.
