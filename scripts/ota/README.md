# scripts/ota — over-the-air update rigs

Drivers for over-the-air updates (`docs/adr/2026-10-06-ota-update-protocol.md`),
the C6's and, with `--chip esp32s3`, the S3's (S3 updates plan, M7 P10). Each
script's header is its full usage. Without `--chip` every script runs the
C6's commands as it always did.

| Script | What |
|---|---|
| `build-image.sh [--chip …] <out> <version> [features]` | One split image of this tree, packaged with its OTA files (`ota-manifest.json`, `core.z`, `engine.z`) into `<out>/ota`, plus `merged.bin`, `core.bin`, `engine.bin`, `split.json`. The scenario images (`just test-emu-c6-ota`, `just test-emu-esp32s3-ota`) and the walk's `--after-update` are built with it. The S3 has one door (the ESP-IDF bootloader): its `merged.bin` is that door, and there is no `merged-lp.bin` |
| `emu-cut-sweep.sh [--chip …] <x> <y> <out> <cut>...` | Power cuts on the emulated board (`lp-cli emu run --rom-up-flash … --ota-cut-after N`), then a recovery run per cut; the scenario tests U3/U4 are the gated form. `OPS=1` cuts at the Nth program or erase anywhere in `factory` instead, torn (`--flash-cut N:calibrated:1,range=<factory>,then=stop`). The S3's verdict reads the newest hello after the last boot, not the `[CORE]` line (`docs/defects/2026-10-22-the-s3-loses-its-core-line-after-an-updates-reset.md`) |
| `make-engine-less.sh [--chip …] <MAC> [--dry-run]` | Leaves one real C6 without its engine: reads the boot records over USB, finds the engine header where the active core puts it, and erases that one sector, so the board boots core-only and the next host restores it. Resolved by MAC, refused while someone else holds it on the desk's `board` lease. The iPhone walk's step 3 (`docs/reports/2026-10-06-ota-iphone-walk.md`) |
| `s3-desk-sitting.py usb\|cuts --as <holder> --images <dir> --out <dir> [--dry-run]` | The S3's update desk sitting on FS3 (M7 P11). `usb` runs on any port: known-good, X's boot transcript, a push, X → Y, the boot records, a heal, X → Y-dies, an RTS reset, known-good again. `cuts` first proves the port really cuts FS3's power: the MAC must leave the bus, and the boot after must read POWERON (`rst:0x1`). Leaving the bus alone proves nothing, because a cut of one USB twin does that too. Then real cuts through `board power-cycle`. Writes one console per step and `summary.md` |
| `hw-power-cut.py` | Real power cuts on a C6 behind a switchable hub (`uhubctl`, both twins of a VIA hub), board resolved by MAC. Flashes X before every cut. A desk sitting: back the board up first and restore it after. **A PASS demands proof the power went**, on both chips: a heartbeat reading `resetReason: power-on` after the cut, or Y's boot count restarted (RTC memory only a power loss clears; an update with no cut adds at least three boots to X's). A converged update without that proof is its own verdict, `NO-CUT-PROOF`: it prints loudly, the run exits 1, and it is never counted a pass. A half cut (one USB twin switched) leaves the bus, rejoins it and resumes the update exactly like a real one, so only the board's own word can tell (`docs/defects/2026-10-23-board-power-cut-one-usb-twin-and-every-check-passed.md`). Each cut ends with a capture to Y's first heartbeat (`cut-*.fin.txt`) for the boot count. `--chip esp32s3` also reads Y's hello instead of the `[CORE]` line (`docs/reports/2026-10-24-s3-power-cuts-desk-walk.md`). The verdict (`judge`) is replayed against captured logs, a real no-cut update among them, by `just lint-power-cut-verdict` (`test_hw_power_cut.py`, in `check-lint`) |

`lp-cli emu run --host-link --ota-offer <dir>` and `lp-cli link capture
<port> --ota-offer <dir>` are the hosts underneath (`lp-fw/builds/README.md`);
`lp-cli link capture blepipe:<port> --ota-offer <dir>` is the same host over
Bluetooth, through `spikes/ble-lab/pipe.html` (that README's "Updates over
Bluetooth").
