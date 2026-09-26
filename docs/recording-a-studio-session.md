# Recording a Studio session (`?record=`)

When something goes wrong between Studio and a board (a hang at "Syncing
project…", a silent kick back to Devices, a stream that loses a frame),
the session recorder writes down everything the page saw, so an agent can
replay it afterwards. It records to a small receiver on your own machine;
nothing is uploaded anywhere.

## 1. Start the receiver

```bash
cargo run -q -p lp-cli -- record serve --out recordings
```

It prints two lines you need:

```
  sink     http://127.0.0.1:63298/ingest
  add to a Studio URL:  ?record=http%3A%2F%2F127.0.0.1%3A63298%2Fingest
```

The port is picked fresh each run (`--port N` pins one). Leave it running.

## 2. Open Studio with the flag

Add the printed query to any Studio address. Production works from the same
machine: an `https://lightplayer.app` page may post to `http://127.0.0.1`.

```
https://lightplayer.app/?record=http%3A%2F%2F127.0.0.1%3A63298%2Fingest
http://127.0.0.1:<studio port>/?emu=ws://…&record=http%3A%2F%2F127.0.0.1%3A63298%2Fingest
```

- Chrome may ask to let the page reach devices on your local network.
  Allow it, or nothing is recorded (the badge then counts lines not sent).
- A pill at the bottom-left reads **● Recording → 127.0.0.1:63298** for as
  long as the page is recording. It shows on every page. If it says "N lines
  not sent", the receiver is not reachable.
- The sink must be on this machine or the local network (`localhost`,
  `127.*`, `[::1]`, `10.*`, `172.16–31.*`, `192.168.*`, `*.local`).
  Anything else is refused and nothing is sent. The pill then reads
  "Recording refused: … is not on this machine or local network".
- Moving around inside Studio keeps the flag in the address, so a refresh
  keeps recording. Each page load is a new session, and the receiver writes
  each session to its own file: `recordings/<YYYYMMDD-HHMMSS>-<session>.jsonl`.

Then use Studio normally and reproduce the problem.

## 3. Read it back

```bash
cargo run -q -p lp-cli -- record timeline recordings            # newest session
cargo run -q -p lp-cli -- record timeline recordings --all      # every session, in order
cargo run -q -p lp-cli -- record timeline <file> --kinds route,error,request,open
cargo run -q -p lp-cli -- record timeline <file> --since 340 --wire raw
```

One line per event, timed from the session start:

```
 +17.657s  REQ      c11#1073741824 access.list sent
 +17.658s  WIRE  →  serial:1  accessList id=1073741824 39 B
 +17.672s  WIRE  ←  serial:1  accessList id=1073741824 28 B packed
 +17.680s  REQ      c11#1073741824 access.list answered in 23.0 ms
+404.743s  ROUTE    /p/playful-choker-…?on=mac:… → /devices   (browser-nav)
+404.906s  WIRE  ←  serial:1  error.error id=1090520149 48 B packed
+404.926s  REQ      c15#1090520149 project.read failed in 85.0 ms: server error: Project not found: handle 1
```

- `--wire frames` (the default) joins each link's chunks back together and
  decodes them: JSON Pack frames and `M!{json}` lines become a message kind
  with its `id`/`seq`/`fin`, board log text is shown after `|`, and a frame
  that does not decode is shown as `!! undecodable frame`, never dropped.
  `--wire raw` shows each chunk's size and first bytes; `--wire off` hides
  them.
- When nothing at all happened for more than 2 s while a request was waiting
  for its answer, a line says so: `… 5.0 s silent (request c1#41 project.read
  outstanding)`. That is what a hang looks like.
- A request that never got an outcome (its caller gave up and dropped it) is
  listed at the end as `no outcome`.
- A request's single final answer frame is folded into its outcome line;
  streamed frames, and frames that did not match their request, are shown.

## What each kind means

The file is JSON Lines. Every line has `seq` (the page's own order) and `t`
(epoch seconds). Device-event lines may also carry `session` and `endpoint`.

| kind | what it is |
|---|---|
| `session` | the first line: recording id, build (version, sha, channel, branch), browser, page URL |
| `route` | an address change, with `from`, `to` and a `reason` (`boot`, `browser-nav`, `slug-heal`, `hint-heal`, and `open-ended: …` for the kick back to Devices after an open) |
| `command` | a Studio command sent from the UI (name and a short summary, not a dump) |
| `action` | an action's result: `ok`/`failed`, how long it took, the error |
| `open` | a project-open stage (`on-device:uploading`, `idle`, `failed`, …) |
| `error` | an error- or warning-level Studio log entry |
| `toast` | a toast the page showed |
| `journal` | the device model's journal (links, identity, activities) |
| `state`, `flow`, `pool`, `mgmt`, `sweep`, `sync`, `anomaly` | device-model transitions (as in the device-trace contract) |
| `request` | a client request: `sent`, each `frame` of its answer (with `disposition`: `matched`, `stale`, `prior-owner`, `uncorrelated`, `server-originated`) and its `outcome` (`answered`, `failed`, `timed-out`, `cancelled`) with latency. `(conversation, id)` names one request |
| `wire` | raw bytes on a transport (`serial`, `ble`, `emu-tab`), per port and direction, base64 |

## Not captured

- Raw clicks and key presses. Only the commands they turn into are recorded.
- Anything before the page loaded, including a previous page load. That is
  its own session file, if it was recording.
- Web Serial's first reads while a port opens (the reset-and-read), the
  firmware flashing flow, and the in-browser simulator worker.

## Privacy

A recording is **unredacted**. It holds all device traffic, including the
access handshake (the keys that unlock a board), and your projects. Keep
recordings on your machine and don't share them publicly.

## Phones

Not supported yet. A phone on `https://lightplayer.app` cannot post to
`http://<your Mac's LAN address>` (the browser blocks that as mixed content).
`record serve --lan` listens on the network for a Studio served over plain
http on the LAN. Recording from prod on a phone needs https or an upload
path, which is future work.

## Also

- Studio still keeps its smaller device trace in `localStorage` (the current
  session, and the one before the last refresh), whether or not `?record=`
  is on.
- Scripts that drive Studio (`scripts/device-scenario.mjs`,
  `scripts/emu/emulated-lane.mjs`) use the same flag with their own receiver.
