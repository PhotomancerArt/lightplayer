# ADR: Studio updates boards over the update channel, on every link

- **Status:** Accepted
- **Date:** 2026-10-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** `lp2025/2026-10-05-0820-ota-studio-ble-updates` (M7 of the OTA
  roadmap `lp2025/2026-10-03-1330-ota-firmware-updates`). PR-1 (P1–P6, the
  card, the model and the driver, dormant) and PR-2 (P7–P9, live over USB and
  the emulator, this ADR's PR). PR-3 (Bluetooth) amends this ADR only if
  Bluetooth differs.

## Context

A board running an update-capable core speaks the update protocol on
lp-link channel 3 (`2026-10-06-ota-update-protocol.md`, "ADR 2"): it reports
its manifest (`M`), takes a new core and engine in chunks it asks for, backs
its engine up to the host on request, resumes a cut transfer, and rolls a
failed core back by itself. The host half of that protocol is
`lpa-update`'s `UpdateDriver` (Part A). Release firmware and its update
files are distributed by M5 (`lp2025/2026-10-04-0757-ota-firmware-distribution`;
it records its decisions in that plan, not in a separate ADR).

Until M7, Studio's only firmware verb was the USB flash: Lasting (two-click
arm), esptool-shaped, the port held alone, the board's files at risk
whenever the layout moved, and disabled over Bluetooth ("Firmware updates
need USB"). M7 puts the update protocol behind the card. This ADR records
how Studio carries it, who decides what, and what a person sees and
presses.

## Decisions

### 1. Update traffic is a routed channel, not a borrowed wire (DS1)

Channel-3 bytes travel through the device model's link contract as their
own pair, `LinkEvent::Update(bytes)` / `LinkCommand::SendUpdate(bytes)`,
beside channel 1. The effects layer's link pump routes them to the
device's update host (`lpa-studio-core` `update_host.rs`), the way it
routes a conversation's replies; the fold never sees them. Nothing pauses
the pump for an update, so channel 1 (the hello, heartbeats, the card's
own reads) keeps the fold honest throughout.

*Rejected:* running the update as a coarse effect that borrows the wire,
as the flash does. The flash needs the port alone (esptool); lp-link
multiplexes channels, and a borrowed wire would freeze the card for a
minute or more.

Every lp-link transport Studio drives carries channel 3 the same way:
Web Serial (a real port and the `?emu=` shim), the emulator tab, and
Bluetooth once it is on lp-link (PR-3). `LinkInfo::carries_update_channel`
says so per transport; a sim link carries none.

### 2. The device model stays serde-only (DS2)

`lpa-devices` mirrors the few face-deciding facts of the board's manifest
(`UpdateFacts`: state, version, target, transfer progress and owner,
crashing, refused build) plus the verbatim manifest JSON. `lpa-link`, the one
place the two vocabularies meet, decodes `M` and the hello's `firmware` with
`lpc-update` and fills the mirror (`device_link/update_facts_mirror.rs`).
`lpa-studio-core` parses the verbatim JSON when it needs the driver's
`decide()`.

*Why:* `wire.rs`'s mirror discipline ("mirror only what a face or a verb
turns on"), and no dependency cycle (`lpa-update` → `lpa-devices`).

### 3. One `Update` activity, run in legs; one driver per device (DS3)

The reducer in `lpa-devices` owns the stage and percent on the card, the
wait for the board to come back across each reset, deadlines and Cancel
(only while backing up). The effects layer holds one `UpdateDriver` per
device across legs; the driver resumes from the board's manifest on every
link. A leg is one link session.

**A board's reset need not close the port** (found by the walk). Over a
transport that does not re-enumerate — the emulator's door, and the bench
C6 on a Mac, whose port stayed open through all three of an update's
resets — the restart is an lp-link session reset on an open port. The pump
reports it (`UpdateHost::on_link_reset`), the driver goes down there, and
comes back up when the board speaks on the new session: its `M`, or a hello
that announces channel 3 (`on_hello`). The leg carries on.

**A card that merges into a remembered board keeps its update.** Identity
reconciliation can fold the card an update runs on into a remembered
record mid-update; the roster records the merge (`Roster::merged_into`) and
the update's markers and driver follow it (`UpdateHost::follow_merge`).

### 4. Heal and finish start themselves; installs are offers (DS4, N9)

When a board's facts say it waits for its engine, or it holds this
Studio's half-finished transfer, the controller starts the `Update`
activity with no click; neither is an offer. What a person presses, and
what it costs (`ActionConsequence`):

| Row | Button | Level |
|---|---|---|
| Update available | `Update` (another build: `Install Y`) | Routine |
| Keeps crashing | `Reinstall`, `Other version…` | Routine; Lasting when the version is older than the board's |
| A version Studio can't get | `Install Y` | Routine; Lasting when older |
| Backing up | Cancel | Routine |
| Needs USB once (a pre-update board), over USB | today's USB flash | Lasting |

An over-the-air update to a newer version is Routine because it is backed
up first, resumed after any cut, and rolled back by the board itself on a
failed start: nothing is lost by pressing it. The agent may press it.

### 5. The route: over the air on every link, USB included (QY1, N12)

A board takes the update channel when **all three** hold: it can update
over its link (its manifest names a split layout and a chip this Studio
knows), the link carries channel 3, and **this Studio holds an
update-capable build** of what it would install
(`device_update_route.rs`):

| board can | build can | link | route |
|---|---|---|---|
| yes | yes | carries channel 3 | over the air |
| yes | no | USB | today's flash (Lasting) |
| yes | no | Bluetooth | no install offer; the card says why |
| no, or no channel 3 | — | any | today's flash |

USB goes over the air too: one path, and the safer one (backup, resume,
rollback, files kept). The switch is one constant
(`USB_UPDATES_OVER_THE_AIR`), so "no" would turn only the USB case back.

*The build half* exists because a local Studio may carry a single image —
`just studio-dev` builds one by default (23 s against 50 s for the split,
warm, M2 Max) — and a single image has no update files. Such a Studio has
no own-build facts, so it keeps the flash over USB and offers nothing over
Bluetooth, with the line "This Studio's build can't update the board
wirelessly. Update it over USB, or from a Studio that can."
`LP_FW_IMAGE=split` opts a local Studio into the split build; release and
deploy bundles are always split.

### 6. This Studio's own build ships beside its merged image (DS5, DS10)

The bundle adds the split package's `ota-manifest.json`, `core.z` and
`engine.z` beside `manifest.json` and the merged image, in the same
`firmware/<target>/` directory (C6: ~1.84 MB more, fetched only when an
update runs). *(Amended 2026-10-06: they first shipped one level down, in
`firmware/<target>/ota/`, where lightplayer.app's firmware lookup —
`/firmware/<target>/<release>/<file>` — answered before the bundle; see
`docs/defects/2026-10-06-the-bundles-ota-files-are-shadowed-by-the-firmware-lookup.md`.)* `core.bin` and `engine.bin` are not
shipped: Studio slices them out of the merged image by the package
manifest's `split` offsets, and checks every piece against
`ota-manifest.json` (the package hash, the core's layout, the image)
before it offers a byte (`bundled_own_build.rs`). The card's standing is
computed from the build's *facts* alone (`HostBuildFacts`, a few hundred
bytes, read at start), never from its 5 MB of bytes.

### 7. Busy takeover (DS8)

A board another link is updating is "Another device is updating it":
Studio asks it again (`Q`) every 3 s and takes over — finishing or
restoring — when the board stops reporting an owner. No protocol change.

### 8. Speak channel 3 only to a board that announced it (DS9)

A board's hello carries `firmware`, or it sent `M` on its own; until one of
those, the link refuses `SendUpdate` with a note and the leg ends
`NeedsUsb`. A board without channel 3 would never acknowledge a reliable
frame on it, and lp-link would stall the whole link.

## Consequences

- The card's firmware zone has a second path, and the flash keeps its own
  verb for the cases above; the USB flash is still how a pre-update board
  gets its one USB visit.
- Every Studio deploy carries ~1.84 MB more in the bundle, none of it
  fetched by a page that never updates.
- Rates are Studio's own: the update's terminal line names bytes, seconds,
  KB/s per leg and each reconnect's time. Measured over the `?emu=` shim on
  a Mac, the backup leg is bound by the shim's model of a Mac's serial path
  (255 B a read, a read every 16 ms: ~12 KB/s); `?emu-tty=none` lifts it to
  ~54 KB/s on the emulator. A real Chromium's rate is a desk number, not
  this.
- `USB_UPDATES_OVER_THE_AIR` and `ServeConfig::USB` are the two dials USB
  has. The second was measured at 63.1 s (ahead 1) against 58.5 s (ahead 4)
  for a whole lp-cli update on the bench C6 and left at 1. Once the board
  erased blocks ahead, ahead 4 was worth 44.8 → 37.4 s, and it is now 4
  (2026-10-06; ADR 2's "a USB update in half the time" amendment).

## Amendment (2026-10-08): over Wi‑Fi

Plan `lp2025/2026-10-06-2249-ota-wifi-updates`, PR B (P7, P8), on PR A's
board (the LAN link serves channel 3, core-only answers its own key lookup).

- **A LAN link carries channel 3.** `lan_link_info` says
  `carries_update_channel`; the WebSocket session sends and drains channel 3
  through the same `LinkPortService` as USB and Bluetooth (DS9's
  announced-first rule; a core-only board's unasked `M` is the
  announcement). A link through the relay still carries none, and its card
  keeps `UPDATE_NOT_OVER_WIFI_YET`.
- **Three links, one word each.** `UpdateLink` is USB, Bluetooth or Wi‑Fi,
  read off the endpoint (`UpdateLink::of_endpoint`) by the update host
  (`ServeConfig::LAN`, the narration's word), the card's facts and the
  no-click start. Wi‑Fi routes as Bluetooth does (§5): over the air, or
  "can't update over Wi‑Fi from this Studio" when this Studio's build has no
  update files.
- **The tier is the key's.** Over Bluetooth and Wi‑Fi the decision reads
  the access layer's granted tier (`UpdateLink::update_tier`), so a
  play-only user on the LAN is told the update exists and offered nothing
  the board refuses; a cable is trusted.
- **The LAN reconnects itself.** A board's reset closes its socket; the
  page's session redials by itself, so between legs the activity waits for
  that redial and opens the new link (`reconnects_itself` for `lan:`, as for
  Bluetooth), never knocking on the dropped one. Once a hello has said the
  board's MAC, a session dialled at an IP also tries the board's
  `lp-xxxx.local` socket when the IP has gone unanswered for 10 s
  (`lan_name_fallback`); the session keeps its URL, so the board keeps its
  card.
- **Busy is said once.** A board's close 1013 (its one LAN slot taken) is a
  drop in words and a slow redial (2, 5, 15, then 30 s), and a redial to a
  busy board is not announced until the board answers, so a second tab or
  lp-cli beside Studio does not flap the roster. A person's "Connect over
  Wi‑Fi" hears "Busy with another connection — try again".
- **A release from before Wi‑Fi updates** (W6) announces channel 3 in its
  hello, but its LAN link ignores it. Over Wi‑Fi, a leg whose first `Q` hears
  nothing for 5 s ends `NotOverWifi`; the card then says the board updates
  over USB or Bluetooth until it has been updated once, and offers nothing
  there. No hello field, no wire bump.
- **Update messages wait for room.** `LinkPortService` holds channel-3
  messages the link's 24 KiB send ring refuses (`Full`) in an outbox and
  moves them in as acknowledgements free it — lp-cli's host already did.
  The LAN's 8 ahead of raw 4 KiB restore chunks overran the ring and a
  restore stalled at 1 % (defect
  `docs/defects/2026-10-08-a-studio-update-burst-past-the-send-ring-was-dropped.md`).
- **Measured.** Emulated (`lp-emu:esp32c6:t1+net=lan`, `just walk-ota-emu
  --lan`): X → Y with a backup, 3 resets, backup 15 s, core 9.7 s, engine
  12 s. Silicon (FC6 fixture-c6 on the test access point, headless Chrome on
  a Mac, no USB host): X → Y with a backup 78.7 s from the press (backup
  31 s at 58 KB/s, core 12 s, engine 15 s); with the backup cached and the
  board's power cut mid-core, 50.4 s. lp-cli on the same desk (PR A):
  58.8 s with a backup. The terminal's "reconnected in" read 1.0 s for
  every Wi‑Fi reconnect, emulated and on silicon, a power cut's included,
  so over Wi‑Fi it does not measure the board's time away (not yet
  explained; the likeliest reading is a reset's socket that stays silent
  until the board answers again, and the activity's 1 s reopen knock).

## Amendment (2026-10-08): through lightplayer.app's relay

Plan `lp2025/2026-10-06-2249-ota-wifi-updates`, PR C (P9, P10), on the
board half in `docs/adr/2026-10-06-ota-update-protocol.md`'s amendment of
the same date. Studio's relay half stays behind `?relay=` (#1031's flag).

- **A relayed link carries channel 3.** `relay_link_info` says
  `carries_update_channel`, and the relay's session drains it through the
  same `LinkPortService` as the LAN. With every link Studio reaches a board
  by now carrying the channel, `UPDATE_NOT_OVER_WIFI_YET` is retired: a
  board reached through the relay is offered the over-the-air update, never
  the USB flash verb's "Firmware updates need USB", which stays the reason
  for flash, factory reset and a board that can only be flashed.
- **A fourth link, `UpdateLink::Relay`.** Said "Wi‑Fi" in the update's words
  (the card's link line already says "via lightplayer.app"); served
  `ServeConfig::RELAY` (4 ahead, the backup in 1 KiB pieces, as Bluetooth:
  the board's relay leg takes 2 KiB at a time, and a whole 4 KiB read-back
  chunk would hold the board's frame buffer for round trips). Not measured
  through a real relay yet.
- **A relayed session rides through a reboot.** The relay ends a board's
  sessions when its leg drops (4410, redialled), and until the board is
  back it answers "board offline" (4404), which otherwise ends a relay
  session (#1031: each redial spends the page's tries at the relay). Each
  update message sent through the relay holds its session for 2 min
  (`RELAY_UPDATE_HOLD_MS`, longer than the activity's 90 s gap); while held,
  4404 redials after 3 s and the relay's "slow down" (4420) after 30 s, the
  time one try takes to come back. The activity waits for the relayed link
  like a LAN or Bluetooth one (`reconnects_itself` for `relay:`).
- **A board whose firmware predates updates through the relay** announces
  channel 3 and then ignores it there: W6's 5 s silence ends the update
  `NotOverWifi`, and through the relay the card says **"Update nearby
  once"** — "This board updates via lightplayer.app after one update
  nearby." — since its own Wi‑Fi may already do. `FIRST_RELAY_UPDATE_RELEASE`
  is `None` until PR C is in a release.
- **Measured, emulated only** (`lp-emu:esp32c6:t1+net=lan`, `just walk-ota-emu
  --relay`: a local lp-cloud-server, the board's leg through the virtual
  LAN's uplink, headless Chrome): X → Y with a backup, 3 resets (backup 21 s,
  core 30 s, engine 26 s, wall); the relay dropping the board's leg mid-core
  (one "board offline" ridden through, back in 4.0 s, the core resumed at
  its record); a power cut mid-engine (resumed). No number here is the
  internet's.

## References

- `docs/adr/2026-10-06-ota-update-protocol.md` (ADR 2: the protocol and
  the firmware)
- `docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`
  (ADR 1: the split image)
- `docs/adr/2026-10-01-offer-tree-and-consequence-levels.md` (levels)
- `lp-app/lpa-update/README.md`, `lp-core/lpc-update/README.md`
- `lp-fw/builds/README.md` (what the bundle carries; single vs split)
- `scripts/emu/walk-ota-emu.mjs` (`just walk-ota-emu`)
