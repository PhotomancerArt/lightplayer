# `walks/` — deterministic host scripts

One file per walk: the host half of a conversation with the machine, in the
script grammar `--uart0-script` and `--usb-script` share, replayable in guest
time alone.

| file | what it is |
|---|---|
| `examples-basic.script` | `lp-cli upload examples/basic` — 13 wire frames, from the hello request to `projectRead` |
| `examples-meteor.script` | `lp-cli upload examples/meteor` — 14 frames, the two-shader project the spike report §11.2 measured its heap ledger on |

**A walk is not a link.** The same file replays on either: `after "<line>"`
matches what a host *on the link the run used* received, so `upload-walk`
runs `examples-basic.script` over the spike's UART0 workaround and
`upload-walk-usb` runs the identical file over the modelled USB-Serial-JTAG
socket, and their heap ledgers come out equal to the byte (M6 P5). Which
flag carries the file is the payload's `link`, not the walk's business.

**The projects these were captured against no longer exist at those paths.**
Both were captured at firmware `d6cfaa205` against `examples/basic` and
`examples/meteor` **as they stood at that commit**, because that is what the
spike report's §5.3 and §11.2 figures are of. The catalog reorganisation has
since moved them to `catalog/patterns/` with an `effect/` subdirectory, so
the wire paths differ and the figures would not compare. To re-capture,
materialise the originals first:

```sh
mkdir -p target/walk-projects
git archive d6cfaa205 examples/basic examples/meteor | tar -x -C target/walk-projects
```

## Where they come from

Not by hand. `scripts/emu/upload-walk.sh` runs the real client against the
machine over a socket and `uart-tcp-proxy.py` transcribes both directions;
`scripts/emu/walk-script.py` reads the host records back out and writes the
script. So the bytes are the ones `lp-cli` actually sent — nothing here
parses or rebuilds a wire message, and the framing rule
(`lpc_wire::json::to_serial_line`, PR #538) stays lp-cli's.

To regenerate after a change to the project or the client:

```sh
cargo build --release -p lp-emu-esp32c6 -p lp-cli
# WALK_LINK=usb puts the machine's half on the link the product ships
# (`--usb-sj tcp:` + `--usb-host attached`); the default is UART0.
WALK_LINK=usb scripts/emu/upload-walk.sh <fw-esp32c6.elf> target/walk \
    target/walk-projects/examples/meteor
scripts/emu/walk-script.py target/walk/walk.uart.bin \
    --note "Firmware: …, Project: …, Link: …" \
    -o lp-emu/esp/lp-emu-esp32c6/walks/examples-meteor.script
```

## Why the script and not the live client

The live walk is honest and **not** deterministic: `lp-cli`'s requests land
where the host's wall clock puts them against the guest's emulated clock,
which is what the spike report's §7 and §11.3 spend most of their diff on.
The script replaces that clock with the guest's own — each request waits for
the answer to the one before it — and two runs are byte-identical.

Two pacing facts are baked into the generated file, and both are the *host's*
behaviour, not the device's:

- **64 bytes per write, 2 ms apart.** UART0's RX FIFO is 128 bytes; at
  921,600 baud that is 1.39 ms of wire, and the firmware's reader takes 64
  bytes per turn of its server loop. A host that streams a 739-byte request
  without pausing overruns the FIFO, the part drops the byte, and the device
  logs `dropping unparseable N B M! line`. The desk walk this is modelled on
  went through a bridge board, which paces itself.
- **Each request waits for the previous answer**, matched on `"id":<n>,` —
  every wire frame carries its request's id, and the answer is the only
  place that id appears in the device's own output.
