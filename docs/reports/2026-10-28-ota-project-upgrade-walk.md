# A firmware update upgrades the board's projects: the crossing walks

**Status: walked on the emulator, 2026-10-28, by an agent (mapping rebuild
M3 P12).** Every number and line below is emulated (`lp-emu:esp32c6:t1`,
`lp-emu` at `79ed065d4`); none is a hardware claim, and emulated time is
never a gate here. The decision this walks is
[`docs/adr/2026-10-28-firmware-updates-upgrade-the-boards-projects.md`](../adr/2026-10-28-firmware-updates-upgrade-the-boards-projects.md).

**The headline, and one finding the plan did not predict.** A board on the
last published format-11 release, holding format-11 projects, ends lit with
its projects at format 12 and the same WS281x frames as before, with no
click, when this build reaches it **over a cable** (lp-cli's update host on
the update channel; Studio's USB flash), and a power cut anywhere in the
stage is finished by the next host. **Over Wi‑Fi, Bluetooth and the relay,
this Studio offers no update to that board at all**: it speaks wire 45, this
Studio speaks 46, and Studio's only update for an older-wire board is the USB
flash ("Firmware updates need USB"). The project stage itself runs on all
four links (walked from a board already on this build). See "Found".

## What was crossed

| | |
|---|---|
| X (the fielded board) | release **2026.10.27-5** (`2026.10.27-5+fd2d9ad73997`, commit `fd2d9ad7399757402f464e45536a7ba8b588ff77`, wire 45, project format 11), fetched from `https://lightplayer.app/firmware/esp32c6-4mb/2026.10.27-5/…` by `scripts/ota/fetch-release-image.sh`, every file checked against the release's own hashes; never rebuilt here |
| Y (lp-cli scenarios) | this tree at `eb94ddf0a`, `scripts/ota/build-image.sh … b1b1b1b1` (`b1b1b1b1+eb94ddf0aa4d`, wire 46, format 12) |
| Y (Studio walks) | this Studio's own split package at `cfdaff8b4` (`cfdaff8b4+cfdaff8b4fe3`) |
| The projects | `lp-cli/tests/fixtures/ota-cross-format/v11`: `eaves` (lpa-upgrade's v11 `eaves-wired`: a manual patch, the front run reversed, D10 × 60 and D9 × 50, its shader replaced by `projects/test/shader-oracle`'s static position-coded one) and `headband` (v11 `fiber-headband`), written onto X's chip over X's own wire as plain `filesystem` `write` requests (the file requests did not change across the bump); `eaves` is the startup project (first by path) |

## lp-cli scenarios (`just test-emu-c6-ota`, X1 and X2)

`lp-cli/tests/emu_ota.rs`, the board hosted in process on its USB link with
lp-cli's own update host on channel 3 and `finish_board_projects` (what `emu
run --ota-offer` and `firmware install` run) as the last stage.

- **X1, cross-format — PASS.**
  `X1 PASS (lp-emu:esp32c6:t1): release 2026.10.27-5 (2026.10.27-5+fd2d9ad73997) (format 11) -> Y with the stage: `[WARN] fw_esp32_common::boot: Boot: auto-load failed for /projects/eaves: Core error: Failed to load core project: parse /project.json: unsupported project format 11 (expected 12); regenerate or upgrade the`; projects: upgraded “eaves” to format 12 · projects: upgraded “headband” to format 12; frames on GPIO18 (180 B) and GPIO20 (150 B) equal before and after; a second stage: `projects: every project is at format 12``
  The test also asserts the hello's `startupRefused` is `{ project: "/projects/eaves", found: 11 }` and `projectFormat` 12, and that both runs' frames are lit (not black).
- **X2, cut-upgrade — PASS.** The flash as it stood before each of the
  stage's 37 requests (folded to 11 distinct flashes) booted cold, offered Y
  again, then a fresh host's stage. Rows, verbatim:
  - `PASS cut-before=1:listAvailableProjects (a) boot: refused /projects/eaves (format Some(11)) stage: projects: upgraded “eaves” to format 12 · projects: upgraded “headband” to format 12 frames equal`
  - `PASS cut-before=18:filesystem.writeChunkDeflated (b)` … `21:filesystem.deleteFile (b)` — the same words (inside the startup project's batch: the board came back with the eaves wholly at 11)
  - `PASS cut-before=23:loadProject (c) boot: refused nothing stage: projects: upgraded “headband” to format 12 frames equal` (and `27:filesystem.read (c)`)
  - `PASS cut-before=34…37 (d)` — inside the headband's batch, `upgraded “headband”`
  - `X2 PASS (lp-emu:esp32c6:t1): 11 distinct flashes cut across the stage's 37 requests (release 2026.10.27-5 (2026.10.27-5+fd2d9ad73997)): every one boots Y, refuses what is old by name, and a fresh host finishes it with the frames equal`
- Both run through the `lp` door only: a release ships one door (its own
  bootloader), so the recipe's `idf` pass skips them with a line.

## The cut sweep at the new stage (`OPS=lpfs scripts/ota/emu-cut-sweep.sh`)

Cut N = the Nth program or erase in `lpfs` during the whole run — the
update and its project stage — torn per lp-nor-sim's `calibrated` model
(`--flash-cut N:calibrated:1,then=stop`). Recovery = the same host run;
verdict = a third run that must say `projects: every project is at format
12` and decode on GPIO18 and GPIO20 the last frame X lit before the update.

- Census: `160 in-range op(s) counted`. **Swept N = 1…159: 159 / 159 PASS.**
  Cuts 1–31 fall in the update (X's and engine Y's own boot writes,
  `in=update`), cuts 32–159 in the project stage (`in=stage`).
- Cut 160 never fired: the uncut run's census counts one op more than the
  cut runs make, and with nothing to cut its run reached the 595 s bound
  twice (the board running the eaves at ~140 fps the whole time). Not
  investigated.
- Rows, verbatim (`target/ota-cross/sweep-lpfs/all.tsv`, not committed):
  `PASS cut=32 in=stage after-cut=(proven) build b1b1b1b1+eb94ddf0aa4d reachable recovery-served=D 0 chunk(s) / 0 B; Z 0 chunk(s) / 0 B final=build b1b1b1b1+eb94ddf0aa4d engine 1931230 B ; stage: upgraded “eaves” to format 12;upgraded “headband” to format 12;; after: every project is at format 12 frames equal`
  and
  `PASS cut=159 in=stage … stage: upgraded “headband” to format 12;; after: every project is at format 12 frames equal`.
- **Found while building it:** a `then=stop` cut inside the stage became a
  channel-1 transport error and the hosted run stepped on past the cut, on a
  flash taking no more writes (the board rebooting, its store saying `no
  space left on device`, for 300 emulated seconds). Fixed in `EmuLinkHost`:
  a stopped board stays stopped (`cfdaff8b4`).

## Studio walks (`walk-ota-emu`, headless Chrome, the release bundle)

Each waits for the board's own words (its console on the door, or its USB
link's capture on the Wi‑Fi lanes) as well as the card. Screenshots and
reports are under `target/walk-ota-emu/<lane>-<stamp>/` (not committed).

| Step | Link | Result | The board's and Studio's words |
|---|---|---|---|
| `cross-format` | `?emu=` (USB) | **PASS** | Studio flashed the older-wire board: `Wrote 3506176 bytes (2038771 compressed) at 0x0`; Y said `Boot: auto-load failed for /projects/eaves`, then `Project loaded: eaves`; Studio: `upgraded “eaves”, “headband” to project format 12` |
| `cut-upgrade` | `?emu=` | **PASS** | power cut (the chip power-cycled, the cable out until Studio saw it go, back in) the moment the page logged `projects: DeviceId(1)'s project stage starts (engine reads format 12)`; Y said the refusal ×2, then `Project loaded: eaves`; Studio: `upgraded “eaves”, “headband” to project format 12`, no click — "before the startup project's batch committed: the board came back refusing the eaves again" |
| `found-older` | `?emu=` | **PASS** | Y (already this build) said `Boot: auto-load failed for /projects/eaves`, then `Project loaded: eaves` with no press; no `[OTA] offer` |
| `cross-format` | `--tab` | **PASS** | as on `?emu=` (a USB flash): `Wrote 3506176 bytes … at 0x0`, the refusal ×1, `Project loaded: eaves` |
| `cross-format` | `--lan` | **FAIL** | the card: `older LightPlayer` · `Firmware updates need USB.`; the board's link: `hello · proto 45 · seeed/xiao-esp32-c6 · another wire: only its version and board were read`; no `update-firmware` offered |
| `cut-upgrade` | `--lan` | **NEVER RUN** (cannot run: no update is offered over Wi‑Fi, above) | — |
| `found-older` | `--lan` | **PASS** | Y said the refusal, then `Project loaded: eaves`; Studio: `upgraded “eaves”, “headband” to project format 12` (the secure LAN link, its real login) |
| `cut-older` | `--lan` | **PASS** | power cut (`ok power-cycle cyc=94560001 us=591000`) as the stage started; Y refused ×2, then `Project loaded: eaves`; finished with no click |
| `cross-format` | `--relay` | **FAIL** | the card listed the board through the account (`Firmware v2026.10.27-5`, `lightplayer.app … online`, a `Connect` button) but stayed quiet; the board's relay leg said `routes=0` (no browser route opened) |
| `found-older` | `--relay` | **PASS** (with `--fresh`) | Y said the refusal, then `Project loaded: eaves`; Studio: `upgraded “eaves”, “headband” to project format 12`. Without `--fresh` the walk's remembered offline card of the same MAC shadowed the live one (the stage itself ran and finished in the page's log both times) |
| `cross-format` | `--ble` | **FAIL** | as on `--lan`: `older LightPlayer`, `Firmware updates need USB.` |
| `found-older` | `--ble` | **PASS** | Y's hello said `"startupRefused":{"project":"/projects/eaves","found":11}`, then `Project loaded: eaves`; Studio: `upgraded “eaves”, “headband” to project format 12`. ⚠️ `?ble=emu` proxies the board's USB link: it proves the card and the transport's channel-1 session over the Bluetooth path, not radio, and not access |

The stage's time per link was not measured.

## Found

1. **No over-the-air crossing in Studio.** The update across the bump
   reaches a fielded board only over a cable today. Studio files a board on
   another wire as `OlderLightPlayer` and updates it with a USB flash
   (`lpa-studio-core` `device_flash.rs` `firmware_verb`, "flash to update;
   the project stays"). Over Wi‑Fi, Bluetooth and the relay, no flash is
   possible and no channel-3 update is offered: "Firmware updates need USB".
   Every wire bump does this to every fielded board reached without a cable,
   and project format 12 rides wire 46. Planning's fact 3 assumed "only
   channel 3 is spoken to it". The stage itself is fine on every link once
   the board runs this build; what is missing is the update before it.
   **A ruling for the director**, not fixed here.
2. **On USB the crossing is a flash, not the update channel.** So Studio's
   `?emu=` walks prove the flash route plus the no-click stage (Q107/Q108),
   not "backing up, updating" on channel 3. The channel-3 crossing over USB
   is lp-cli's (X1, X2, the sweep).
3. **A board reset under a live link mid-stage** (a power cycle that keeps
   the shim's port open) leaves the stage to the next connection: the
   no-click stage runs once per link window, by design
   (`project_stage_controller.rs`). A USB-powered board's real power cut
   takes the port with it, which is what `cut-upgrade` walks.
4. **The emulated run stepped past a power cut** inside the stage (above;
   fixed).

## What this does not prove

No silicon, no radio (no airtime, no BLE controller, no Wi‑Fi PHY), no real
relay or internet (a local `lp-cloud-server`), no Chromium USB stack. The
play-tier path (a play link shows the words and writes nothing) was not
walked end to end: it is proven at the stage (`lpa-update` `projects_sim`)
and in `lpa-studio-core`'s tests. The S3 was not walked.

## Run it

```bash
just test-emu-c6-ota "" x1            # X1 (and x2): fetches the release, builds X and Y
OPS=lpfs JOBS=10 DROP_FLASH=1 scripts/ota/emu-cut-sweep.sh target/ota-cross/x-release target/ota-cross/y <out> $(seq 1 30)
just walk-ota-emu --steps cross-format          # one step per invocation keeps each under ten minutes
just walk-ota-emu --steps cut-upgrade
just walk-ota-emu --lan --steps found-older     # and cut-older; --relay --fresh, --ble
```
