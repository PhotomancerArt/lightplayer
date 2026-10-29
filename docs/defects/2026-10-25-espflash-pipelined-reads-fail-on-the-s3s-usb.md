---
status: wontfix
found: 2026-10-24      # how: desk sitting (FS3 power cuts, backing up lpfs); confirmed 2026-10-25
area: host tooling — espflash 3.3.0 `read-flash` over the S3's USB-Serial-JTAG on macOS
class: misattributed-symptom   # it read as FS3's flash going bad
related:
  - docs/reports/2026-10-24-s3-power-cuts-desk-walk.md ("Also seen")
  - docs/reports/2026-10-25-s3-update-checks-desk-walk.md
  - scripts/ota/s3-desk-sitting.py (its records read)
---
# espflash's pipelined flash reads fail on the S3's USB

**Symptom** — `espflash read-flash` of FS3's 1.5 MB `lpfs` failed with
"Corrupt data, expected 1000 bytes but received fdb" (or `d45`, `eff`) and
wrote an empty file. 128 KiB pieces mostly worked. It looked like FS3's flash
returning bad data.

**It is the host tool, not the board.** On 2026-10-25, with FS3 (XIAO
ESP32-S3 Plus, 16 MB) on the desk Mac's VIA hub, the same 1.5 MB region at
`0x610000`:

| reader | result |
|---|---|
| `espflash read-flash` (default `--max-in-flight 64`), twice | fails after ~6 s, empty file |
| `espflash read-flash --max-in-flight 8` | fails after ~14 s |
| `espflash read-flash --max-in-flight 1` | 1,572,864 B in 29 s, sha256 `8f60f24c…faa16` |
| `lp-cli hardware lpfs save` (one read) | 1,572,864 B in 31 s, sha256 `8f60f24c…faa16` |

The two good readers agree byte for byte. The stub's reply is short by a
fraction of a 4 KiB packet once more than one packet is in flight: the reply
stream loses bytes on the way in, the familiar shape of macOS's USB CDC path
under a burst. The flash is fine.

**Why `wontfix`** — the fault is in a third-party tool's default, and the
product never reads flash this way: Studio and `lp-cli` read through their own
flasher (`lp-cli hardware lpfs save`, the migration's backup), which read the
whole region cleanly. The desk scripts that use `espflash read-flash` read
8 KiB at most.

**Workaround** — for a large read on an S3, use `lp-cli hardware lpfs save
--port <port> --out <dir>` (it also writes the partition table and a backup
ZIP), or `espflash read-flash --max-in-flight 1`. `scripts/ota/s3-desk-sitting.py`
reads the boot records with `--max-in-flight 1`.

**Also seen once, same sitting:** after a `read-flash` of `0x0..0x20000`, FS3
did not return to its firmware (330 s with no frame on the link) until
`espflash reset`, which brought it back with `resetReason: user-reset` and its
RTC boot count kept. The script now resets the board after every read.
