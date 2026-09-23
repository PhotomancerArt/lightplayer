# ble-lab — remote-controlled Web Bluetooth

`spikes/serial-lab`'s shape, for BLE: the human opens one page and clicks
**Join** once; after that the agent drives the link over HTTP. Built
2026-09-23 for the BLE spike in the `ble-remote-control` vision, against
`fw-esp32c6`'s `test_ble` harness (`just fwtest-ble-esp32c6`).

Web Bluetooth needs a user gesture only for the chooser. Reconnecting to the
same `BluetoothDevice` needs none, so the page reconnects by itself after a
drop and records every connect and disconnect with its uptime. That timeline
is the evidence for link stability.

The server also owns the board's USB console through
`scripts/emu/tty-capture.py`, which never asserts DTR/RTS (the reset sequence
on Espressif native USB). Every `/cmd` response carries the console lines
that arrived while it ran, so one call shows both sides.

```bash
python3 -u spikes/ble-lab/server.py          # prints its port (dev-port.sh ble-lab; BLE_LAB_PORT overrides)
P=36666                                      # whatever it printed
curl -s -X POST localhost:$P/serial -d '{"dev":"/dev/cu.usbmodem113301"}'   # start the console reader
curl -s -X DELETE localhost:$P/serial        # release the port before flashing
```

Human: open `http://localhost:$P/` in Brave (with
`brave://flags/#brave-web-bluetooth-api` enabled) or Chrome, click **Join**,
pick `LP-BLE-…`, and leave the tab in front.

```bash
cmd() { curl -s -X POST localhost:$P/cmd -d "$1"; echo; }
cmd '{"op":"status"}'
cmd '{"op":"idle","ms":30000,"timeoutMs":40000}'   # hold the link, report drops
cmd '{"op":"echo"}'                                # rtt
cmd '{"op":"burst","n":200,"timeoutMs":30000}'     # board -> page throughput
cmd '{"op":"writes","n":200,"timeoutMs":30000}'    # page -> board (upload direction), board-counted
cmd '{"op":"send","text":"hello"}'
cmd '{"op":"disconnect","reconnect":true}'         # drop and let the page reconnect
cmd '{"op":"auto","on":false}'                     # stop auto-reconnect
cmd '{"op":"eval","js":"return lab.S.events"}'     # escape hatch
curl -s localhost:$P/serial?n=40                   # the board's console tail
curl -s localhost:$P/log?n=40                      # page telemetry + server lifecycle
```

`spikes/ble-lab/scripts/nus-probe.py` does the same checks with `bleak` from a
terminal. It can't run from an agent shell on macOS: TCC aborts a process
whose responsible app doesn't declare `NSBluetoothAlwaysUsageDescription`
(exit 134, no prompt). Run it from Terminal.app if you want the host-stack
numbers.
