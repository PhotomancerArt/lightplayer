---
status: open
found: 2026-10-21      # test (lp-emu:esp32s3:t1, --flash-cut, lp-nor-sim calibrated)
area: fw-esp32s3 lpfs (littlefs) — the first boot's format of a blank `lpfs`
class: write-ordering
related:
  - 2026-10-08-littlefs-rust-relocation-cut-leaves-lpfs-unmountable
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M7 P5 found it; P9 moves the S3 to the tree store)
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

**What is not known** — whether silicon tears that program the way
`lp-nor-sim`'s `calibrated` model does (the tear models were calibrated on
the C6's part); whether other tears/seeds of the format do the same; and
whether the mount's free-space answer is littlefs reading a half-written
superblock or the firmware's mount path accepting it. Not chased: M7 P9
moves the S3's `lpfs` to the tree store, which replaced littlefs on the C6
for this class of failure, and a board with no user files is reflashed
there (ruling L7).

**Close when** — P9's tree store is the S3's `lpfs` and the same cut sweep
over its first boot shows every second boot able to write, or this
reproduces on silicon and gets its own fix.
