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
its device store says so, so provision the board over USB first.

> **This page's wire mode predates lp-link and does not talk to current
> firmware.** Since wire proto 37 (PR #880,
> `lp2025/2026-09-28-1445-ble-on-lp-link`) every GATT write and
> notification is exactly one lp-link Datagram frame (4-byte header, the
> JSON/packed payload, 4-byte CRC-32C), the board's link opens at the
> subscribe and sends its hello after the handshake, and a Prepare Write on
> RX is refused. `index.html`'s `wireWrite`/`onWireBytes` (join on newline,
> write in raw 180-byte chunks), the `M!`-string builders in `lab.py`,
> `m4-desk-check.py` and `nus-probe.py`, and `tapstat.py`'s classifier all
> assume the old `M!` lines. The spike-image modes above (`echo`, `burst`,
> `writes`, `idle`) are unaffected.
>
> Two tools speak the new link today:
>
> - **Studio itself** — the desk walk below. It is the product's own code
>   (`lpa-link`'s `browser_ble.js` and `ble_link_port.rs`, pinned by
>   `lpa-link/tests/browser_ble_conformance.rs`).
> - **The frame pipe**: `pipe.html` here (served at `/pipe`; a page that
>   only moves frames between Web Bluetooth and a WebSocket) with
>   `lp-cli link capture blepipe:<port>` hosting the lp-link session in the
>   terminal. It is the tool for updates over Bluetooth and for an
>   unattended soak ("Updates over Bluetooth" below); the scripted battery
>   below waits on being ported to it.

The old wire mode's commands, kept for the record (they need the porting
above before they work again):

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

`scripts/m4-desk-check.py` runs M4's whole desk battery after one Join:
refusal before login, the 10 s unauthenticated drop, wrong password and
backoff, login, request round trips, granted parameters, and an idle window
(the knob round trip is `lab.py`'s `knob_rtt`, not part of it). It
prints one JSON record per check. It does not judge ESP-NOW loss; that comes
from the `[COEX]` lines over the window it prints.

```bash
BLE_LAB_PORT=$P python3 spikes/ble-lab/scripts/m4-desk-check.py --password desk-lab [--idle-min 10] [--skip-drop]
```

For connection-parameter experiments, add `desk_ble_params` to the features
(never shipped). The board then reads its requested parameters from
`/.lp/ble-exp.txt` at boot, so a run changes them with a file write and a
reboot, not a reflash:

```text
interval_us latency timeout_ms [idle_after_ms idle_interval_us idle_latency]
```

With no file, the board asks for the shipped 15 ms / latency 0 / 4 s. This
is how the M4 runs K and L measured latency 4 against latency 0 (the plan's
`spike-results.md`; ruling in `docs/adr/2026-09-24-ble-transport.md`,
Amendment).

## No phone, no human: the Mac's own Chrome as the central

`scripts/cdp-central.mjs` answers the Join chooser over the Chrome DevTools
Protocol (`DeviceAccess`), so an agent can run the whole battery. It was used
for M4's Run J. Launch Chrome in the **background** with a scratch profile. It
must never be a foreground window.

```bash
open -g -n -a "Google Chrome" --args --remote-debugging-port=9333 \
    --user-data-dir=<scratch>/chrome-ble --no-first-run --no-default-browser-check \
    http://localhost:$P/
node spikes/ble-lab/scripts/cdp-central.mjs --debug-port 9333 --page localhost:$P \
    join --prefix LP- --timeout-ms 60000
node spikes/ble-lab/scripts/cdp-central.mjs --debug-port 9333 --page localhost:$P \
    js 'location.reload(); 1'          # a fresh page before a re-Join
```

What was learned doing it (2026-09-24, Chrome 153, macOS):
- **The chooser lists the name macOS has cached**, not the advertised one.
  The board advertised `LP-PLAYFUL Choker` while the chooser offered
  `LP-BLE-b48c` (the spike image's name), and later `LP-b48c`. Match with
  `--prefix LP-`, not `--name` — and with **two boards on the desk, by
  `--id`** (2026-10-02: both desk boards answered `LP-PLAYFUL Choker…`).
  `cdp-central.mjs list` prints every device the chooser offers with its
  id, then cancels the chooser, so nothing connects to the wrong board.
- **Close `chrome://bluetooth-internals` before joining.** While that tab
  was open, every chooser reported one empty device list and never updated;
  the first attempt after closing it found the board in 1.4 s. This is
  correlation from one session: the cause is not proven.
- A second connection from the same Mac was **not tried**. CoreBluetooth
  gives each peripheral one link per host, shared between apps, so a second
  Chrome profile would most likely not open the board's second slot. That is
  expected, not measured.
- `requestDevice` needs the gesture. The Join click runs through
  `Runtime.evaluate` with `userGesture: true`, and nothing else is required.

## Updates over Bluetooth: `lp-cli link capture blepipe:`

An over-the-air update of a C6 over its Bluetooth link, with no human: the
terminal runs lp-link, the login and the update (`lpa-update`'s driver, the
same one Studio runs), and `pipe.html` in a backgrounded Mac Chrome is the
radio. The page only moves frames: one lp-link frame per GATT write and per
notification, as Studio's own Bluetooth link does.

**Say what a rate was measured with.** The central here is Mac Chrome over
CDP, which is **not Bluefy on an iPhone**: a different Bluetooth stack, a
different MTU (iOS 185 against macOS 247) and a different write path. Every
rate names its central and the distance ("Mac Chrome via CDP, board 1 m
away"); a phone's number comes only from a phone. The pipe also writes
WITHOUT response (S5c's best: window 16, four chunks ahead, unpaced), where
Studio writes every frame WITH response, so a pipe rate is not Studio's rate
either. On the OTA spike's sitting (S5c, 2026-10-02, hub board) the same
settings gave 5 to 34 KiB/s from one run to the next, with nothing in our
code changing: quote the runs, not the best.

What `lp-cli` does (`lp-cli/src/commands/link/blepipe_capture.rs`,
`ble_pipe_host.rs`, `engine_login_gate.rs`):
- **Every Bluetooth connection is a new link.** The board opens its end at
  the subscribe and drops it with the connection, so the page's `up` starts
  a fresh lp-link session under a new nonce, and `down` ends it. The update
  driver sees the link go and a new one come, and asks `Q` again. That is
  how the run carries on across the update's **three resets** (each is a
  dropped connection; the page reconnects to the same device, no chooser).
- **Host window 16, `--ota-ahead` 4** by default (S5c). The board's
  core-only advertises a receive window of 32; nothing is tunable here
  beyond `--ota-ahead`.
- **Logins.** A running engine takes channel 3 only at the tier its server's
  login holds for the link, so with `--ota-password` the capture logs in on
  channel 1 first (the board's hello says an engine runs) and starts the
  update on the verdict. Core-only says `M` unprompted when a link comes up;
  there the update starts at once, and the core's own login (`L` on channel
  3) is the driver's, with the same password, when the board answers
  `N`/`A`. Without a password nothing logs in, which is the refusal check.

### The offers

X and Y are two split images of this tree, packaged with their OTA
directories exactly as `just test-emu-c6-ota` builds its scenarios:

```bash
scripts/ota/build-image.sh target/ota-ble/x a0a0a0a0
scripts/ota/build-image.sh target/ota-ble/y b1b1b1b1
# each: merged.bin, package/ (the USB image), ota/ (what --ota-offer reads)
```

Put X on the board over USB first (`scripts/ota/hw-power-cut.py` does it
the same way: `espflash write-bin --chip esp32c6 --port <port by MAC> 0x0
target/ota-ble/x/package/*-merged.bin`), and back the board up before the
sitting. Resolve the board by MAC (`scripts/emu/board-port.py <MAC>`), never
the first port.

### A run

```bash
python3 -u spikes/ble-lab/server.py            # serves the pipe at /pipe; prints LAB, its port
PIPE=$(scripts/dev-port.sh ble-pipe)           # this worktree's port for the capture
cargo build --release -p lp-cli

# 1. The host. It waits for the page, and stops at the driver's last word.
target/release/lp-cli link capture blepipe:$PIPE \
    --console target/ota-ble/run1.txt --seconds 900 \
    --ota-offer target/ota-ble/y/ota --ota-cache target/ota-ble/cache \
    --ota-password '<the board password>' \
    --exit-on '[host-ota] done:' 2> target/ota-ble/run1.err

# 2. The central: Chrome in the BACKGROUND, a scratch profile, a debugging
#    port of Chrome's own choosing (never a pinned one), read back from the
#    profile.
open -g -n -a "Google Chrome" --args --remote-debugging-port=0 \
    --user-data-dir=<scratch>/chrome-ble --no-first-run --no-default-browser-check \
    "http://localhost:$LAB/pipe?ws=ws://127.0.0.1:$PIPE"
CDP=$(head -1 <scratch>/chrome-ble/DevToolsActivePort)
C="node spikes/ble-lab/scripts/cdp-central.mjs --debug-port $CDP --page localhost:$LAB/pipe"
$C list --timeout-ms 8000          # every board the chooser offers, with its id; picks nothing
$C join --id '<the board id>'      # by id: two desk boards share a name prefix
$C js 'pipe.S'                     # the page's counters: connects, drops, timeouts, writes
```

`--ota-no-z` sends every chunk raw (`D`); without it the offer's encoding 1
(`Z`) goes wherever it is smaller. `?ack_every=N` on the page's URL sends
every Nth frame as a write WITH response: an experiment knob, off by
default (S5c measured it slower).

**Reading it.** The console (`--console`) is the whole story in order:
`[pipe] …` (the page: connects, timeouts, write failures), `[host-ble] …`
(each connection up and down, the engine's login), `[host-ota] …` (the
driver: the board's manifest, the decision, each stage, the end) and the
board's own wire messages as `M!{…}`. The numbers:
- `[host-ble] Bluetooth connection N down (…); 85.2 s up, 1234567 B served
  (14.1 KiB/s); its link — … resent …` (console and stderr) — **the
  connected rate**, per connection, with that connection's link counters;
- at the end, on stderr: `connection N up at T s for D s: …` — each
  connection, which puts **the reconnects and their times** in one list;
- `update over Bluetooth: … B served (… KiB/s) while connected, N
  reconnect(s)` — payload over time connected, S5c's measure;
- `ota — … D n chunk(s) / B; Z n chunk(s) / B; …` — what went raw and
  encoded. **The `Z` ratio** is (D bytes + Z bytes) over the core and engine
  lengths the `[host-ota] offering …` line prints.

Count **Chrome restarts** by hand; nothing else sees them.

The board's own log lines (`[CORE] core @… build …`) never travel on a radio
link: read them over USB with a second capture,
`lp-cli link capture <port by MAC> --console usb.txt --seconds 900`. That
capture is a link host too, and any link coming up confirms a trial core,
so a run that must prove **Bluetooth** confirms the trial goes with no USB
attached at all.

### The refusal, then the login

- **Refused:** the same capture with **no** `--ota-password`, on a board
  whose access holds a password with Play and Author set to Password. A
  running engine refuses the backup's read-back and the offer (`N`/`A`): the
  run ends `[host-ota] done: Stopped(NeedsEngineLogin)` with nothing erased,
  and X still running.
- **Granted:** with `--ota-password` the console says `[host-ble] the
  engine's login granted Edit`, then the update runs. After the first reset
  the board is core-only and asks again (`N`/`A`); the driver logs in to
  the core itself (`[host-ota] the board asks for a login`), and the run goes
  on. A wrong password: the engine's login is refused (`[host-ble] the
  engine's login was refused …`) and the run ends as with none; refused by
  core-only, it ends `Stopped(LoginRefused)`.

### When Mac Chrome wedges

Chrome's Web Bluetooth on macOS can wedge after a few board restarts: the
chooser stops offering the board, or a `connect()` hangs (the desk walk
below, and S5c: 1–3 Chrome restarts in most runs; once the chooser cancelled
itself for about ten minutes, then recovered). The page bounds every
`connect()` at 12 s, disconnects and retries, which covers a hung connect.
A chooser that offers nothing is not covered: quit **that** Chrome (the one
with the scratch profile — `pkill -f 'user-data-dir=<scratch>/chrome-ble'`,
never the desk's own Chrome), open it again the same way, and `join` again.
The capture keeps running and takes the new page as the new connection.

## Studio itself, with the Mac's Chrome as the central

The same backgrounded Chrome and `cdp-central.mjs`, pointed at a Studio dev
server on **this worktree's** port instead of the pipe. This is how an agent
walks Studio's own Bluetooth path on a real board before a person does.
Nothing is shimmed: no `?ble=emu`, no `?emu=`, so the page's
`navigator.bluetooth` is Chrome's. (`?ble=emu` proxies the emulated board's
USB stream: it proves Studio's UI, not the radio and not access.)

```bash
just studio-dev                     # prints Studio's URL: the source of truth, never a pinned port
STUDIO=http://127.0.0.1:<the port it printed>
open -g -n -a "Google Chrome" --args --remote-debugging-port=0 \
    --user-data-dir=<scratch>/chrome-studio --no-first-run --no-default-browser-check "$STUDIO/"
CDP=$(head -1 <scratch>/chrome-studio/DevToolsActivePort)
C="node spikes/ble-lab/scripts/cdp-central.mjs --debug-port $CDP --page ${STUDIO#http://}"
# the home page's Bluetooth square: exact word, inside Connect a board
BT='[...document.querySelectorAll("#home-connect-board button")].find((b) => !b.disabled && b.innerText.trim() === "Bluetooth")?.click() ?? null'
$C list --click-expr "$BT" --timeout-ms 8000             # the ids, nothing picked
$C join --id '<the board id>' --click-expr "$BT"
$C shot target/ota-ble/studio-1.png                      # the card, as the person would see it
$C click "<a button's text>"                             # press what the card offers
```

- `--click-expr` runs the expression as the press. The Bluetooth square is
  the exact word `Bluetooth` inside `#home-connect-board` (Connect a board,
  never hidden); a bare `--click-text "Bluetooth"` would also press a board
  card's Bluetooth switch, which is why the press is scoped.
- A **locked board** asks for its password on the card. Press the card's
  password button with `click`, then type into the focused field with `js
  "document.execCommand('insertText', false, '<password>')"`, which fires
  the input events Studio listens to, and press its confirm button.
- Read Studio's numbers from **Studio's own terminal line** for the update
  (bytes, seconds, rate, reconnects and their times), not from the pipe's;
  Studio writes every frame with response, the pipe does not.
- **A reload loses the board in Mac Chrome**: `getDevices()` returns
  nothing without Chrome's "Web Bluetooth new permissions backend", so after
  a reload `join` again (Bluefy restores the board with no chooser). Adding
  `--enable-features=WebBluetoothNewPermissionsBackend` to the launch line
  may restore it; that is untested here.
- **Mac Chrome is not Bluefy**, as above: a pre-walk in Mac Chrome is the
  agent's evidence that the flow works on a real radio, not a phone's
  number. The person's walk is on the phone.

**A phone reaching a dev Studio:** Web Bluetooth needs a secure context, and
the phone cannot reach `127.0.0.1`. Serve the dev server on the tailnet over
HTTPS, on a port other than 443 (the perf lab holds 443 on the desk):

```bash
tailscale serve --bg --https=8443 http://127.0.0.1:<studio port>
# phone (Bluefy): https://<desk>.<tailnet>.ts.net:8443/
tailscale serve --https=8443 off              # when done: --bg is a setting, not a process
```

## A phone (Bluefy on iOS): HTTPS over Tailscale

Web Bluetooth needs a secure context, and `localhost` doesn't reach a phone.
Expose the lab on the tailnet, on a port other than 443: the perf lab
(`scripts/emu/lab/`) already owns 443 on the desk.

```bash
tailscale serve --bg --https=8443 http://127.0.0.1:$P
# phone: https://<desk>.<tailnet>.ts.net:8443/  → Join → LP-BLE-… (spike image) or LP-… (product)
tailscale serve --https=8443 off              # when done: --bg is a setting, not a process
```

## Coexistence: BLE beside Wi-Fi/ESP-NOW (`test_ble_coex`)

`test_ble_coex` is `test_ble` plus Wi-Fi/ESP-NOW brought up the way the
product's radio driver does it (`esp_radio::wifi::new`, channel 11,
broadcast), with `esp-radio/coex` on. Each board broadcasts a
sequence-numbered frame at a fixed rate and prints a `[COEX]` line every 2 s,
with both directions' counters (the peer reports its view inside its frames).
Loss over a window is Δ`rx_last_seq` − Δ`rx` (peer → this board) and
Δ`peer_last_seq` − Δ`peer_rx` (this board → peer). A sequence up to 1,000
below the high-water mark counts as `rx_dup`, not as a reboot. The images used
in M2's sitting (`08c6167b3`) predate that fix: there, a duplicate zeroed `rx`,
so board → peer is computed per 2 s interval, skipping any interval where a
counter fell. `rssi_avg`/`peer_rssi` from those images are unsigned bytes
(237 = −19 dBm). Build-time knobs:
`LP_COEX_BLE=0` (no BLE: the control, and the peer board),
`LP_COEX_RF_SWITCH=0` (leave the XIAO's RF switch undriven), `LP_COEX_HZ`
(default 50).

```bash
cd lp-fw/fw-esp32c6
LP_COEX_BLE=0 cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 \
    --no-default-features --features esp32c6,test_ble_coex
espflash flash --chip esp32c6 --partition-table partitions.csv --flash-size 4mb \
    --after hard-reset --port <port by MAC> ../../target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6
```

`spikes/ble-lab/scripts/nus-probe.py` does the same checks with `bleak` from a
terminal. It can't run from an agent shell on macOS: TCC aborts a process
whose responsible app doesn't declare `NSBluetoothAlwaysUsageDescription`
(exit 134, no prompt). Run it from Terminal.app if you want the host-stack
numbers.

## The desk walk: Studio over Bluetooth on lp-link (PR #880's gate)

What the walk answers: does Studio connect, edit and Play over Bluetooth on
the new link, are the link counters sensible, and is there no drop beyond
Bluefy's known phantom one. Nothing before this walk has proven the radio:
host tests, the conformance suite and `?ble=emu` all stop short of it
(`?ble=emu` rides the emulated board's USB link).

1. **The emulated walk first, on this branch** (the rule is emulator first,
   then hardware). With a Studio dev server running from the branch's
   worktree (`just studio-dev`; its printed URL is the source of truth):

   ```bash
   just walk-ble-emu
   ```

   All 18 steps should pass, including the four drops under Play: `drop`
   (the board away for seconds; the cable is the emulated board's power, so
   it restarts), `blip` (the radio drops and the page reconnects at once),
   `phantom` (Bluefy's) and `range` (out of range with the radio link still
   up and quiet; the curtain rises when Play's once-a-minute read times
   out). `range-editor` takes the same quiet drop under the editor first,
   where a sync failure's text would be drawn. No drop may show a request's
   raw timeout. Its idle numbers are emulated: shape, not a
   silicon claim. The board boots the packaged whole chip
   (`{merged},kind=rom-up`; the split image's ELF alone boots core-only).
   `WALK_RECORD_SINK=<an lp-cli record serve sink>` records the session,
   and the page's console lands in `target/walk-ble-emu/page-console.log`.
2. **Put the branch's firmware on the board.** Open the same dev server's
   Studio, connect the board over USB, and take **Update firmware** (the
   image the dev server packaged from the branch: hello `proto 37`). Or from
   a terminal: `just flash-fw-esp32c6 <port by MAC>`. A board on proto 37
   no longer talks to lightplayer.app's Studio (proto 36, Bluetooth on `M!`
   lines) until that deploy catches up, so flash the release image back
   afterwards if the board must work with production.
3. **Unplug USB and connect over Bluetooth**: Devices → add → Bluetooth →
   `LP-…`. Log in if the board is locked (an open board needs nothing).
   Expect the card to identify in a few seconds. Lock the board for at
   least one pass (a password, Play and Author set to Password): the
   resume after a drop has to log in again on the new link, and that is
   what the 2026-10-05 desk check found broken. Type the password the
   first time (with "Remember" ticked), on a browser this board holds no
   key for: that is the path the second fix is for.
   **Mac Chrome: a page reload does not bring the board back by itself.**
   `navigator.bluetooth.getDevices()` answers nothing without Chrome's
   "Web Bluetooth new permissions backend" flag, so after a reload connect
   it again with the chooser (Bluefy restores it). And Chrome's Web
   Bluetooth on macOS can wedge after a few board restarts (the chooser
   stops offering the board, or a `connect()` hangs): quit Chrome and
   reopen it.
4. **Edit**: open the project in the editor, change a slot and a shader
   line, and push. **Play**: turn knobs, switch patterns. Then leave it in
   Play, idle, for 15–20 minutes.
5. **Read the link counters** in the device card's developer view:
   `damaged` should stay at 0, `resets` at 0 apart from drops you caused,
   `resends` low and explained (a busy radio).
6. **Drop it under Play, three ways**: pull the board's power for two
   seconds, ask it to reboot (USB `lp-cli link capture <port> --request
   reboot`, or Studio's restart), and walk it out of range. Each time the
   page must stay on Play behind "Reconnecting…", come back on its own
   (the 2026-10-06 silicon re-check: 7–13 s after a power cut, ~8 s after a
   reboot), with the card "Unlocked by …" again and no unlock sheet, and
   the next knob turn must land. On the board's console, no
   `no login within 10 s — closing` after a drop. If the page goes to
   Devices instead, that is a finding: note the time, keep the board's
   console and, if you can, record the session (`?record=`,
   docs/recording-a-studio-session.md).
7. **Phone (optional, closest to a user)**: the same from Bluefy on iOS,
   over `tailscale serve --bg --https=8443 http://127.0.0.1:<studio port>`.
   iOS negotiates ATT MTU 185, so frames carry 174 B of payload instead of
   180: the board's log line at each connect (`ATT MTU …, frames … B + 8`) says
   which.

The board's console (USB, if left plugged into a second cable, or
`scripts/emu/tty-capture.py`) logs one line per Bluetooth link: the ATT MTU,
the frame size, the link's RAM and the heap at open.
