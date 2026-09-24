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

## Wire mode: the product image (BLE M4)

The product firmware carries the wire itself over the same NUS service, and
advertises as `LP-<project>` (or `LP-<last 4 MAC hex>`), so **Join** now
accepts any `LP-…` name or any board advertising NUS. It starts BLE only when
its device store says so, so provision the board over USB first:

```bash
curl -s -X DELETE localhost:$P/serial                       # release the console
python3 spikes/ble-lab/scripts/provision-access.py --dev <port by MAC> --password desk-lab
curl -s -X POST localhost:$P/serial -d '{"dev":"<port>"}'   # console back (it rebooted)
# … later, to put it back the way it was:
python3 spikes/ble-lab/scripts/provision-access.py --dev <port> --disable
```

Then, after Join:

```bash
cmd '{"op":"wire","on":true}'                                # packets → the line joiner
cmd '{"op":"req","msg":"hello"}'                             # any wire request; reply frames back
cmd '{"op":"req","msg":"listLoadedProjects"}'                # → notPermitted before login (locked board)
cmd '{"op":"login","password":"desk-lab"}'                   # PBKDF2 + HMAC in WebCrypto → loginResult
cmd '{"op":"unsolicited","n":10}'                            # hellos/heartbeats the board sent on its own
```

`scripts/lab.py` has the same as functions (`wire`, `req`, `login`,
`unsolicited`, `knob_rtt`). WebCrypto needs a secure context: `localhost`, or
the Tailscale HTTPS origin a phone uses. The password here is for the lab
only.

For ESP-NOW loss beside the product server, build the desk meter:
`--features esp32c6,server,desk_espnow_meter` (the product image with M2's
`[COEX]` counter in place of the ESP-NOW driver; never shipped). Run it on
both boards and read the `[COEX]` lines from the console as in M2.

`spikes/ble-lab/scripts/nus-probe.py` does the same checks with `bleak` from a
terminal. It can't run from an agent shell on macOS: TCC aborts a process
whose responsible app doesn't declare `NSBluetoothAlwaysUsageDescription`
(exit 134, no prompt). Run it from Terminal.app if you want the host-stack
numbers.
