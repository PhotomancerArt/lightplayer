# esp-radio — LP fork

Vendored from crates.io **esp-radio 0.18.0** (`MIT OR Apache-2.0`, checksum
`23fbff98b06a96b6ce3791ecec5c668524052a068e23aacd23afe17ddba844ce` in
`Cargo.lock` before the patch), verbatim except for the diff below. Patched
in through the root `Cargo.toml`'s `[patch.crates-io]`, the same way
`third_party/esp-alloc` is; the version stays `0.18.0` because the patch
table substitutes a source, never a version. The Apache-2.0 text is
`licenses/Apache-2.0.txt`.

## The diff

### 1. A chained ACL packet is copied whole (`src/ble/npl.rs`, `ble_hs_rx_data`)

The NPL controllers (the C6 among them) hand each received ACL packet to the
host as an `os_mbuf`, and an mbuf can be a **chain**. Upstream copied only the
first mbuf's `om_len` bytes into the packet it queues for the host, so a
chained packet reached bt-hci shorter than its own ACL header says; bt-hci
refused to parse it, and trouble-host's runner failed.

On the desk C6 (2026-09-25, Mac Chrome as the central, ATT MTU 251, one
write per ATT Write Request) exactly the ACL packets of **193–198 bytes** (H4
length without the indicator; ATT values of 182–187 bytes) arrived as two
mbufs; 161–192 and 199–255 arrived as one. Studio's Play-mode knob write is
that size when its value prints short (`4`, `0.25`), which is how a knob
jump took the board's Bluetooth down. The captured packet: header
`02 00 20 c2 00` (handle 0, 194 bytes of data), 190 bytes present, the line's
last 8 bytes (`l}}}}}}\n`) missing.

The fork walks `om_next` and copies every segment (still bounded by the
256-byte packet buffer, which the controller's 255-byte ACL buffers cannot
exceed; a longer chain would be cut with a warning rather than overrun it).

### 2. The parse warning names the packet (`src/ble/controller/mod.rs`, `parse_hci`)

`[hci] error parsing packet: {:?}` printed nothing in a build with
`-Zfmt-debug=none`. It now also prints the packet's length and first five
bytes (indicator and header), which is what separates a short packet from a
garbled one.

See `docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md`.
Both changes are upstream candidates.

### 3. A research switch: the BLE controller's flash-only parameters (`esp_config.yml`, `src/ble/npl.rs`)

`ble_controller_flash_only_params` (C6 only, **default `false`**): when on,
`ble_init` calls the controller blob's own
`esp_ble_controller_flash_only_param_config()` just before
`r_ble_controller_enable(1)`. It is an exported function of `libble_app.a`
that makes three calls (`r_priv_sdk_config_max_aux_offset_set(2000)`,
`r_priv_sdk_config_insert_proc_time_set(500)`,
`r_ble_ll_scan_start_time_init_compensation(500)`), which is what ESP-IDF
5.5.3's `BT_CTRL_RUN_IN_FLASH_ONLY` adds to linking the controller's
`.iram1` into flash. It is the runtime half of esp-hal's
`place_ble_controller_iram_in_flash` (that fork's fifth diff). Off, the
build is byte-for-byte the code above. RAM research experiment E2
(`lp2025/2026-10-09-1203-ram-research`); nothing turns it on.

## Re-syncing with upstream

Copy the new version out of the cargo registry, delete `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig`, then re-apply the
hunks (`grep -n "LP fork" -r src esp_config.yml` finds them) — or drop the fork if
upstream copies chained mbufs itself.
