# `walks/` — host scripts a **payload** owns

The sibling of `lp-emu/esp/lp-emu-esp32c6/walks/`, and the difference between
the two directories is who the file belongs to. The C6's walks are captures of
a *client conversation* — `scripts/emu/upload-walk.sh` transcribes what
`lp-cli` actually sent, and the files are machine-generated. The files here are
a **payload's** host half: the registry names them in a `ChipArm::host_script`
and the configuration driver hands them to `--uart0-script`, so they live
beside the registry that points at them rather than beside a machine.

| file | payload | what it is |
|---|---|---|
| `v3-stop-all.script` | `boot-idle` on `esp32v3` | one request, on the appearance of `[INIT] I/O task spawned`: the classic's heartbeat triple is **elicited**, and this is the eliciting |

## `v3-stop-all.script`'s provenance

Not captured — **transcribed from a committed gate test**, which is the whole
reason it can be checked rather than trusted.
`lp-emu/esp/lp-emu-esp32v3/tests/boot_idle.rs` has driven this exact
conversation since G2: `LAST_LINE` (`[INIT] I/O task spawned`) is the trigger,
`STOP_ALL` is the payload, and `stop_all_script()` delays
`CYCLES_PER_US * 1_000` — one emulated millisecond — between them. The script
file is those three facts in `--uart0-script`'s grammar
(`lp-emu-esp32v3/src/control.rs::parse_byte_script`), and
`a_committed_stop_all_script_matches_boot_idles` in
`lp-emu-validate/src/payload.rs` asserts the file against the constants on
every `cargo test -p lp-emu-validate`. A drift between the gate and the walk is
a red test, not a surprise at the bench.

Why the payload needs one at all, in one paragraph the next reader will want:
`esp32_memory_stats` (`lp-fw/fw-esp32v3/src/main.rs`) is what prints
`[stack] heartbeat:` / `[MEM] free=` / `[JIT] used=`, and `lpa_server` calls it
on a project load, unload or stop-all, or on a client `runtime_status` — never
from the five-second server heartbeat, which calls `heartbeat_memory_stats` and
prints nothing. The C6's `boot-idle` shape (boot, wait 5.5 s, stop on the
`[stack]` line) therefore records an empty transcript on the classic. Lab task
L1 (2026-09-10) asked with these bytes at the desk; this file is the emulated
side asking with the same bytes on the same trigger, so the two sides are the
same **stimulus** and not merely the same image.

The one gap L1 measured, unchanged and not to be widened: the emulated send
lands 1 ms of guest time after the trigger, where a desk host's select loop
polls at 50 ms. That is host latency. It is why nothing in the `timing` class
is compared for this payload.

## A walk is not a link

The same file replays on whichever link the run used — `after "<line>"` matches
what a host *on that link* received. On `esp32v3` that link is UART0 and there
is no other: this part has no USB-Serial-JTAG peripheral at all.
