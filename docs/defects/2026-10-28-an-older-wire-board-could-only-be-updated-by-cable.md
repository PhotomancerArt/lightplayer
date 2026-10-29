---
status: fixed
found: 2026-10-28      # e2e: the mapping rebuild's M3 P12 crossing walks (emulated, wire 45 board, wire 46 Studio)
fixed: this change
area: lpa-link demux + link port service, lpa-devices evidence, lpc-wire hello readers
class: partial-knowledge-loss   # the decode-failure path dropped the manifest and the MAC the hello carried
related:
  - docs/adr/2026-10-06-studio-updates-over-the-update-channel.md (2026-10-28 amendment)
  - lp-core/lpc-wire/src/server/hello_proto.rs
  - lp-app/lpa-link/src/device_link/demux.rs (hello_on_other_wire)
  - lp-app/lpa-link/src/device_link/link_port_service.rs (DS9's announcement)
  - lp2025/2026-10-27-0114-mapping-rebuild (ota-older-wire-update.md)
---
# A board on an older wire could only be updated by cable

**Symptom** — Over Wi‑Fi, Bluetooth and the relay, a Studio offered no
update at all to a board whose JSON wire is older than its own when the
board's hello does not decode at Studio's wire (wire 46 added a required
hello field; a wire-45 board's hello failed to decode). The card said
`older LightPlayer` · `Firmware updates need USB.`; the board's terminal
said `hello · proto 45 · seeed/xiao-esp32-c6 · another wire: only its
version and board were read`; no `update-firmware` was offered. Over a
cable it took the USB flash. Every wire bump that changes the hello's shape
would strand every board reached without a cable — the OTA invariant says
none should be.

**Root cause** — The update rides lp-link's update channel (channel 3),
which never changes with the JSON wire. But when a hello failed to decode,
the demux kept only its `proto` and board id. The board's manifest (its
`firmware`, which is also the board's announcement that it speaks channel
3) and its MAC were in that same hello and were thrown away. Without the
announcement the link refused to send channel 3 (DS9). Without the MAC, a
board reached by radio or relay stayed an unidentified pending link, whose
only firmware verb is the flash. Nothing about the update needed channel 1.

**Fix** — `lpc-wire` reads four more fields off a hello it cannot decode,
each on its own, beside `hello_proto` and `hello_board_id`:
`hello_firmware` (by `lpc_update::BoardManifest`'s own reader, so as stable
as `M` on channel 3), `hello_base_mac`, `hello_device_uid` and
`hello_on_tree_store`. The demux's `HelloOnOtherWire` carries them. The
evidence offers its update facts. `LinkPortService` takes the manifest as
the announcement. Studio's existing route then offers the update over the
air on every link, Routine. The Firmware row's details name the version the
manifest gave. Classification is unchanged: an older LightPlayer, with its
card saying "This board is running old firmware. Update the firmware to
connect and edit."

**Regression coverage** — `lpc-wire` `hello_proto` tests
(`an_older_hello_that_does_not_decode_still_names_its_update_facts` and
two more); `lpa-link` `an_older_hello_with_a_manifest_carries_its_update_facts`,
`an_older_wire_hello_with_firmware_announces_the_channel`,
`an_older_wire_hello_without_firmware_announces_nothing`; `lpa-devices`
`an_older_hello_with_a_manifest_announces_the_update_channel_and_names_the_board`;
`lpa-studio-core` `an_older_wire_board_is_updated_over_the_update_channel_on_every_link`
(presses `update-firmware` by path over USB, Bluetooth, the LAN and the
relay) and `an_older_wire_boards_card_says_to_update_it_and_offers_it_over_wifi`.
The emulated walk `walk-ota-emu --steps older-wire` (each lane), with
`WALK_OTA_X` naming the older X.

**Lesson** — A "could not decode" path is still a "heard" path. When a
peer speaks two protocols and only one of them can drift, the fallback for
the drifting one must keep every fact that belongs to the stable one. Here
the update channel's own manifest was inside the message that drifted.
