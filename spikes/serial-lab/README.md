# serial-lab — remote-controlled Web Serial spelunking

A standalone lab for debugging device-connect behavior on real hardware
when the agent cannot click Chrome's native serial chooser. The human
opens one page, clicks **Grant** once; after that the agent drives every
serial primitive over HTTP while the human watches the live log.

Built 2026-08-21 during the dig2go hardware walk; it root-caused the
hello-gate defect, discovered the CH340 reset sequence, and provisioned
`/hardware.json` over the wire in one evening. Findings from that session:
`docs/defects/2026-08-21-hello-gate-assumes-fresh-boot.md`,
`docs/debt/shared-uart-io-task-starvation.md`, and the dig2go plan dir's
notes.md.

## Architecture

```
agent (curl) --POST /cmd {op,...}--> server.py --SSE--> page (holds Web Serial)
agent        <--result (blocks)----- server.py <--POST /result------
```

`server.py` is stdlib-only Python. The page executes each command and
posts the correlated result; `/cmd` blocks until it arrives (pass
`timeoutMs` for long ops). SSE + POST was chosen over WebSockets purely
to avoid dependencies — the interaction is equivalent.

## Quick start

```bash
python3 spikes/serial-lab/server.py   # http://localhost:29188
```

Human: open `http://localhost:29188/` in a real browser, click
**Grant port**. Agent: everything below.

```bash
curl -s localhost:29188/status                      # server view: page connected?
curl -s -X POST localhost:29188/cmd -d '{"op":"status"}'          # page view
curl -s -X POST localhost:29188/cmd -d '{"op":"adopt"}'           # re-take granted port
curl -s -X POST localhost:29188/cmd -d '{"op":"open","baud":921600}'
curl -s -X POST localhost:29188/cmd -d '{"op":"capture","ms":8000,"timeoutMs":15000}'
curl -s -X POST localhost:29188/cmd -d '{"op":"hello"}'           # send M! hello request
curl -s -X POST localhost:29188/cmd -d '{"op":"frame","msg":"stopAllProjects"}'
curl -s -X POST localhost:29188/cmd -d '{"op":"frame","msg":{"filesystem":{"read":{"path":"/hardware.json"}}}}'
curl -s -X POST localhost:29188/cmd -d '{"op":"sequence","name":"both_high_low"}'  # RESET (see below)
curl -s -X POST localhost:29188/cmd -d '{"op":"signals","dtr":true,"rts":false}'
curl -s -X POST localhost:29188/cmd -d '{"op":"write","text":"raw line"}'
curl -s -X POST localhost:29188/cmd -d '{"op":"close"}'           # release for CLI tools
curl -s -X POST localhost:29188/cmd -d '{"op":"eval","js":"return lab.S.rxBytes"}'  # escape hatch
curl -s localhost:29188/log?n=50                    # page lifecycle telemetry
```

`capture` returns timestamped entries since command start, classified:
`log` (text line), `frame` (`M!` JSON, parsed), `bin` (hex preview),
`meta` (lab events). `buffer {since}` re-reads history without waiting.
`eval` runs an async JS body with `lab` in scope (`lab.S`, `lab.sleep`,
`lab.writeText`, `lab.SEQUENCES`…) — infinitely spelunkable without
redeploying.

## Wire cheat-sheet (lpc-wire over UART, text mode)

- Frames both directions: `M!` + JSON + `\n`, interleaved with plain log
  lines on the same UART.
- Client: `M!{"id":N,"msg":<ClientRequest>}` — variants are camelCase:
  `"hello"`, `"stopAllProjects"`, `"listLoadedProjects"`,
  `{"setLogLevel":{"level":"debug"}}`,
  `{"filesystem":{"write":{"path":"/x","data":"<text or base64>"}}}`.
- Server: hello + responses carry the request id; unsolicited frames
  (boot hello, 5s heartbeat) use `id: 0`.
- App link baud is `DEFAULT_SERIAL_BAUD_RATE` = **921600**; the ROM and
  2nd-stage bootloader log at **115200** (so a boot looks like a binary
  splat at 921600 — that splat then clean `[INIT]` lines IS a boot).

## Hardware lore (hard-won 2026-08-21, CH340 classic / dig2go)

- **Reset from Web Serial**: RTS-only (esptool `hard_reset`) and
  DTR-only pulses do NOTHING. Assert **DTR+RTS together, hold ~120 ms,
  drop both together** (`sequence both_high_low`) → clean power-on reset.
  Native CLI tools (espflash) reset fine via the OS driver.
- **The device is lossy under load** (`docs/debt/shared-uart-io-task-starvation.md`):
  while a project plays (~41 ms ticks), inbound frames >~128 B are
  silently dropped (`FifoOverflowed`) and outbound responses die on TX
  timeouts (`responses=0`). **Send `stopAllProjects` before any big
  write**; pacing alone does not help. Requests answered fine when idle.
- The boot hello arrives ~2–3 s after reset. A running server heartbeats
  every 5 s — connecting mid-stream means heartbeat-before-hello (this
  broke the wizard's gate; see the defect entry).
- One page at a time: two subscribed pages would race to answer
  commands. Close extra tabs.

## Browser gotchas

- **Brave**: Web Serial may need `brave://flags/#brave-web-serial-api`;
  grants are **revoked on page reload** (Chrome persists them). Every
  reload costs the human another Grant click.
- Reloading the page is also the reliable way to free a port the page
  holds (the OS handle closes with the document).
- The Claude-harness Browser pane has `navigator.serial` and can run
  this page too — but cannot click the native chooser, so the human
  grant still happens in a visible browser.

## Known limits

- No binary-safe RX path (lines are decoded as UTF-8 with replacement;
  `bin` entries carry only a hex preview). Fine for the M!/log wire;
  not a protocol analyzer.
- Timing measured in-page is subject to browser throttling of hidden
  tabs; keep the tab visible for timing-sensitive captures.
