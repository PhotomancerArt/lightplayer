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
| `s3-stop-all.script` | `boot-idle` on `esp32s3` | the same request on the same trigger line, because the S3's triple is elicited too (M6 P04b put the printer on the elicitation points, never on the heartbeat) |

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

## `s3-stop-all.script`'s provenance

The classic's file, checked against the classic's file — `the_s3_stop_all_script_is_the_classics_stimulus`
(`lp-emu-validate/src/payload.rs`) asserts that the two carry the **same
single directive**, and the classic's is in turn asserted against
`lp-emu-esp32v3/tests/boot_idle.rs`'s constants, so the S3's walk is two
links away from a gate rather than from an eye. A second assertion checks the
trigger line against `lp-emu-esp32s3/tests/boot_idle.rs`'s `HELLO`, which
pins this image's whole `[INIT]` chain byte for byte: a trigger that is not a
line the firmware prints is a run that waits for ever.

Why one stimulus and not a chip-shaped one: the answer a `boot-idle`
transcript carries is a heap ledger, and two chips asked different questions
produce two ledgers nobody may compare. `stopAllProjects` reaches
`esp32_memory_stats` on both Xtensa chips through the same
`handlers::handle_stop_all_projects`, so the bytes are the same and the
difference left in the transcripts is the chip's.

⚠️ The run this file is written for does not complete yet. Until **M6 P06**
lands the flash controller, the shipped S3 image spins on `SPI1.cmd` after
`[INIT] I/O task spawned` and never reaches the server loop that would read
these bytes (`lp-emu-esp32s3/tests/boot_idle.rs`,
`the_boot_stops_where_p06_begins`). The arm keeps the full sentinel
(`[JIT] used=`) all the same: a sentinel weakened to make today's run pass is
a payload that stops recording the thing it exists for.

## A walk is not a link

The same file replays on whichever link the run used — `after "<line>"` matches
what a host *on that link* received. On `esp32v3` that link is UART0 and there
is no other: this part has no USB-Serial-JTAG peripheral at all. On `esp32s3`
it is the other way round — the console is `jtag-serial` and USB-Serial-JTAG
is the only link the application writes to — so the same directives reach the
machine as `--usb-script` rather than `--uart0-script`, chosen by the driver
from the arm's `Link` and never written into the file.
