# scripts/ota — over-the-air update rigs

Drivers for the C6's over-the-air updates (`docs/adr/2026-10-06-ota-update-protocol.md`).
Each script's header is its full usage.

| Script | What |
|---|---|
| `build-image.sh <out> <version> [features]` | One split image of this tree, packaged with its OTA files (`ota-manifest.json`, `core.z`, `engine.z`) into `<out>/ota`, plus `merged.bin`, `core.bin`, `engine.bin`, `split.json`. The scenario images (`just test-emu-c6-ota`) and the walk's `--after-update` are built with it |
| `emu-cut-sweep.sh <x> <y> <out> <cut>...` | Power cuts on the emulated C6 (`lp-cli emu run --rom-up-flash … --ota-cut-after N`), then a recovery run per cut; the scenario test U3/U4 is the gated form |
| `make-engine-less.sh <MAC> [--dry-run]` | Leaves one real C6 without its engine: reads the boot records over USB, finds the engine header where the active core puts it, and erases that one sector, so the board boots core-only and the next host restores it. Resolved by MAC, refused while someone else holds it on the desk's `board` lease. The iPhone walk's step 3 (`docs/reports/2026-10-06-ota-iphone-walk.md`) |
| `hw-power-cut.py` | Real power cuts on a C6 behind a switchable hub (`uhubctl`, both twins of a VIA hub), board resolved by MAC. Flashes X before every cut. A desk sitting: back the board up first and restore it after |

`lp-cli emu run --host-link --ota-offer <dir>` and `lp-cli link capture
<port> --ota-offer <dir>` are the hosts underneath (`lp-fw/builds/README.md`);
`lp-cli link capture blepipe:<port> --ota-offer <dir>` is the same host over
Bluetooth, through `spikes/ble-lab/pipe.html` (that README's "Updates over
Bluetooth").
