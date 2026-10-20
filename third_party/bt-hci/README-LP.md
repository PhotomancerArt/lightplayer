# bt-hci — LP fork

Vendored from crates.io **bt-hci 0.8.1** (upstream
<https://github.com/embassy-rs/bt-hci>, commit
`c031fd26c71a1568014386a47024933440512e66` per the crate's
`.cargo_vcs_info.json`; checksum
`211713d2e9fb4793ce4360a712c0764264aff6be48932ccf02ca2a331c0436a9` in
`Cargo.lock` before the patch), `MIT OR Apache-2.0`; both licenses are
alongside (`LICENSE-MIT`, `LICENSE-APACHE`). Patched in through the root
`Cargo.toml`'s `[patch.crates-io]`; the version stays `0.8.1` because the
patch table substitutes a source, never a version. Verbatim except that the
packaging files (`.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.toml.orig`) were
dropped, and the diff below.

## The diff: a Command Complete never slices past the caller's buffer (`src/controller.rs`, `ControllerState::complete`)

`ExternalController` gives each pending command a buffer for its return
parameters: the command's declared return size for a sync command, and an
**empty** one for an async command (one bt-hci expects a Command Status for).
When the controller's Command Complete arrived, upstream copied its return
parameters into `event[..data.len()]` — a panic whenever the controller says
more than the command declares.

It does, for two commands. bt-hci (0.8.1, and 0.9.0 too) declares **LE Remote
Connection Parameter Request Reply** (`0x2020`) and its **Negative Reply**
(`0x2021`) as async, with no return, but the Core spec gives both a Command
Complete carrying Status and Connection_Handle — 2 bytes. trouble-host sends
them when a central asks for connection parameters over the link layer, which
Android does on every connection and Mac Chrome and iOS never did; the C6
panicked with `range end index 2 out of range for slice of length 0` each
time (`docs/defects/2026-10-17-an-android-phone-connecting-over-bluetooth-panics-the-c6.md`).

The fork copies what fits, signals the command with that length, and warns
(`[bt-hci] command 0x2021 completed with 2 return bytes, room for 0; extra
ignored`) when it cut anything; the status still reaches the caller. An async
command's answer is its status alone, so the handle is not needed.
`fw-esp32c6` turns on the crate's (already present, optional) `log`
dependency for that line.

Regression: `lp-fw/fw-esp32-common/tests/ble_controller_answers.rs` drives
both commands through `ExternalController` against the controller's exact
answer bytes (it panicked on upstream with the board's own message).
Upstream candidate, with the better fix beside it: declare both commands
sync with `Return = ConnHandle`.

## Re-syncing with upstream

Copy the new version out of the cargo registry, delete `.cargo-ok`,
`.cargo_vcs_info.json` and `Cargo.toml.orig`, then re-apply the hunk
(`grep -n "LP fork" -r src` finds it) — or drop the fork if upstream no
longer slices past the buffer (and declares the two commands' returns). Keep
`Cargo.lock`'s resolution of the fork's ranged dependencies (embassy-sync,
heapless) the same as the registry crate's: a path crate is re-resolved, and
an older embassy-sync or heapless would link a second copy into the firmware.
