# The emulator perf lab

**Yona opens one bookmarked tab on any device, taps Join, and puts it down.
From then on the director decides what that device measures, when, and
against which build, and reads the numbers back with nobody in the loop.**

The lab is a *job queue with presence*: a page holds the device (a phone's
JavaScriptCore on a warm device — the one thing only a page can hold), a
small dependency-free node server on the desk owns the queue and the DD41
protocol (spaced presses, best-of-N, the interpreter row as the thermal
control, the same-press ratio), and the director drives it over HTTP with
one-shot blocking waits. It reuses the browser bench rig in
`scripts/emu/bench-web/` unchanged: a build in the store *is* that rig's
stage directory, and the page starts that build's own `worker.js`.

Everything the server knows is a file under the lab home, so a session can
`ls` it, and a restart loses nothing.

## The home: `~/.photomancer/emu-lab/`

Outside every worktree by rule (D6): `cargo-clean.sh` deletes idle worktree
`target/` directories nightly with no age check, and took a night's uploads
with one once. `LAB_HOME` overrides it (tests use a temp dir).

```text
~/.photomancer/emu-lab/
├── config.json      {port: 41111, domain: null, cooldownMs: 60000, maxResultBytes: 2000000}
├── token            32 hex, 0600, generated on first start
├── builds/<id>/     emu.wasm manifest.json worker.js bench-run.js wasi-shim.js jit-host.js bench-cli.mjs index.html
│                    fw-<slug>.elf -> ../../images/<sha12>.elf       (one ELF copy shared by every build)
├── images/<sha12>.elf
├── jobs/            (P3)
├── results/result-<ISO>.json   the legacy bench-web shape, one per press or manual run
├── devices/<deviceId>.json     identity, last state, last seen
└── log/server.log
```

**The port is pinned at 41111.** The lab is a machine-wide service, not a
per-worktree dev server, so `scripts/dev-port.sh`'s hash does not apply; the
pin is the user-visible exception `docs/process/review-gates.md` allows,
declared in the plan (D14) and repeated in every gate handoff. Edit
`config.json` to move it; `LAB_PORT=0` is for tests only.

## Running by hand

```bash
node scripts/emu/lab/server.mjs                # LAB_HOME=~/.photomancer/emu-lab, port 41111
scripts/emu/lab/lab.sh status                  # the director's view
scripts/emu/lab/lab.sh curl /status            # any endpoint, authenticated
```

The server prints one line on stdout when it listens
(`emu-lab: listening on http://127.0.0.1:41111`); everything else goes to
stderr and `log/server.log`. `LAB_QUIET=1` silences stderr.

## Staging a build into the store

```bash
scripts/emu/bench-web.sh --stage-into ~/.photomancer/emu-lab              # build this tree, stage it
scripts/emu/bench-web.sh --stage-into ~/.photomancer/emu-lab --no-build   # re-stage the existing target/emu-bench-web
scripts/emu/bench-web.sh --stage-into ~/.photomancer/emu-lab --from-stage /tmp/lab-<sha>/target/emu-bench-web
```

The **build id** is `build.short` for a clean tree and
`<short>-dirty-<wasm_sha256[:6]>` for a dirty one (D15); it is also the
`?v=` stamp the page hangs the whole build off, so the F2 (#712) stamp
chain inside a build directory is untouched. The manifest in the store
gains one additive key, `build.id`. ELFs are content-addressed in
`images/<sha256[:12]>.elf` and each build holds a relative symlink (D16);
the server follows the link and refuses anything that resolves outside the
home. Re-staging an id replaces the directory in one rename.

`--from-stage` imports a directory *another* checkout's own
`bench-web.sh --no-serve` produced, which is how a head older than this flag
gets into the store: `git worktree add --detach /tmp/lab-<sha> <sha>`, run
that tree's script `--no-serve`, then this tree's `--stage-into … --from-stage
/tmp/lab-<sha>/target/emu-bench-web`, then `git worktree remove /tmp/lab-<sha>`.
(The reference ELFs are pinned images, identical across heads; point
`LP_EMU_C6_REF_*` at an existing stage's ELFs to skip rebuilding them.)

The rig's desk-engine half runs off a store build unchanged, symlinked ELFs
and all:

```bash
node ~/.photomancer/emu-lab/builds/<id>/bench-cli.mjs --stage ~/.photomancer/emu-lab/builds/<id> --image render-basic --grade t2 --mode interp
```

## Endpoints and the token

One token guards every non-static endpoint (D9, D17): `Authorization:
Bearer <t>` or `?t=<t>` (the `EventSource` form). A miss is `401
{"error":"token"}` with nothing written and nothing logged beyond a counter.
The director reads `~/.photomancer/emu-lab/token` through `lab.sh`; the
phone gets it in the bookmark's URL **fragment** (`/#t=…`), which never
reaches ngrok's or the server's logs — the page moves it to `localStorage`
and strips the hash (P2).

| route | token | what |
|---|---|---|
| `GET /`, `/index.html`, `/lab-page.js` | no | the lab page (this directory's, not a build's) — `no-store` |
| `GET /builds/<id>/<file>` | no | a staged build; `immutable` when the URL carries `?v=`, `no-store` for `manifest.json` |
| `GET /images/<sha12>.elf` | no | the shared ELF store |
| `GET /healthz` | no | `{ok, build, uptimeS}` — the tunnel's liveness probe |
| `GET /events?device=<id>` | yes | SSE presence stream; `hello` on open, a keepalive comment every 15 s |
| `POST /devices/<id>/state` | yes | `{name, ua, cores, deviceMemory, visibility, hasFocus, wakeLock}` → the device file |
| `POST /results/manual?device=<id>` | yes | a hand-taken run in the legacy shape; server names the file, caps the body (413), requires `results[]` (400), adds `manual: true` |
| `GET /status` | yes | devices (with `present`), builds, job counts, result count |

Static paths are resolved with `realpath` and must stay inside the home (or
the script directory for the page); `..`, dot-files and planted symlinks are
404s. Content types: `.html .js .mjs .json .wasm .elf`.

**Present** = at least one open `/events` stream for the id *and* the last
posted `visibility` is `visible`. A hidden tab is connected and useless
(T4), so it is not present.

The server binds `0.0.0.0` — the LAN and the tunnel both reach it — because
the token is the guard, not the interface. There is no general upload: a
result is written only through the routes above, named by the server's
clock, into `results/`.

## The page: what Join does and what it refuses

One page at `/`, the rig's own look, for the phone and for any desk browser
Yona is in front of (Q5). On load it moves `#t=<token>` from the bookmark
into `localStorage` and strips the hash; with no token it shows "Open the
bookmark the director gave you" and nothing else works.

**Join** is one tap, and the tap is what makes the wake lock legal on iOS
(T6): it requests `navigator.wakeLock('screen')`, opens the `EventSource`,
posts the device's state, and turns the page into a status board. After
Join the page runs any press it receives with no second tap (Q2). A reload
re-joins on its own — everything but the lock, which needs a gesture, so
the board shows "Keep the screen on" and rows run either way with
`wakeLock: 'none'`.

**Every row records** `visibilityAtStart/End`, `sawHidden`,
`hasFocusAtStart/End`, `wakeLock` (`active | released | none | unsupported |
denied`), `tainted`, `taintReasons` (D8). A row is tainted when the tab was
hidden at any point or a held lock was released under it; a lock never held
is recorded, not a taint. A press is tainted if any row is; the server lists
it as an exclusion and re-queues it once (D23).

**What the page refuses:** it never starts a row while
`document.visibilityState` is hidden. A `press` received hidden is held, the
server is told (`deferred`), and it starts on the next `visible`. That is
also why an agent-driven tab cannot produce a number here: the harness pane
is hidden, so its press is deferred, correctly.

**Q8, the ngrok interstitial:** every `fetch` sends
`ngrok-skip-browser-warning: 1`; `EventSource` cannot. If a reconnect lands
on the interstitial the stream closes twice in a row and the board says
"Reload the tab once". Observed at the gates, settled at G2.

**Manual Run** keeps the rig's controls over a build picked from the store
and posts to `POST /results/manual?device=<id>` with `manual: true`.

## Protocol (the seam between the page and the queue)

Server → page, over SSE:

```text
event: hello    {serverTime, config:{cooldownMs}, device}
event: press    {job, press, of, build, rows: 'gate-rows' | [{slug,grade,mode,fnBlocks,timeout}], nextPressAt: null}
event: cooldown {job, nextPressAt}          // the countdown the page shows; null clears it
event: queue    {jobs:[{id, state, builds, presses:{done,total}, note}]}   // this device's view
```

Page → server:

```text
POST /devices/<id>/state                {name, ua, cores, deviceMemory, visibility, hasFocus, wakeLock}
POST /jobs/<job>/presses/<n>/result     the legacy payload + {device, deviceName, job, press, buildId, manual:false,
                                         visibility, hasFocus, wakeLock, tainted, taintReasons, deferredMs, failed?}
POST /jobs/<job>/presses/<n>/deferred   {reason:'hidden'}   // received hidden; will run when visible
POST /results/manual?device=<id>        the legacy payload + {manual:true, ...the same taint fields}
```

The page passes `rows: 'gate-rows'` through to the build's own worker as
`preset: 'gate-rows'` (D22) — one definition of the gate rows per build; an
explicit list becomes a `plan` built the way the rig's `buildPlan` does.

## Jobs

A job is `jobs/<id>.json`; its presses land in `jobs/<id>/presses/<n>.json`
(the page's payload) and, when the last press is terminal, `report.json` and
`report.md` beside them. Every press also writes a legacy
`results/result-<ISO>.json` with `job`, `press`, `buildId`, `device` added
(D12).

```json
{
  "id": "j-20260912-0130-a1b2", "kind": "bench",
  "builds": ["86cb2e0", "23a3d3c"],            // one entry = a single build
  "rows": "gate-rows",                           // or [{slug,grade,mode,fnBlocks,timeout}]
  "repeats": 5, "spacingMs": 180000, "device": "any", "ttlMs": 86400000, "retryTainted": 1,
  "note": "P4 vs P3 head", "createdAt": "…", "state": "queued",
  "boundDevice": null, "presses": [{"n": 1, "build": "86cb2e0", "state": "pending"}, …],
  "lastPressEndAt": null, "reportAt": null
}
```

States: `queued → running → done | expired | failed | cancelled`. Press
states: `pending → sent → (deferred) → done | failed`; a `sent` press with no
result after `wallTimeout × rows + 120 s` is lost and re-sent once. Only
`kind: bench` exists; the field is reserved (Q6). `POST /jobs` validates
(1–2 builds that exist in the store, `repeats` 1–20, `ttlMs` ≥ 60 s) and
expands an A/B into `A1 B1 A2 B2 …` (D5).

**The scheduler** ticks every second and on every event. For each present
device with no press in flight, in job order, it sends the first pending
press of the first job that passes four conditions:

1. the job's device filter matches (`any`, the id, or the name);
2. the job is not bound to another device — an A/B job binds to the first
   device that takes its first press and stays there (D20);
3. the device's cooldown has elapsed: `now − device.lastPressEndAt ≥
   config.cooldownMs` (60 s; applies between jobs too);
4. the job's spacing has elapsed: `now − job.lastPressEndAt ≥ spacingMs`,
   measured from the **end** of the previous press (D19) — DD41's "spaced
   by minutes" is thermal recovery.

One press at a time per device. While a device waits on spacing the page
gets a `cooldown` event with `nextPressAt` for its countdown. A tainted
press appends one more press of the same build at the end of the queue
(D23, `retryTainted`, default 1). On restart the server rebuilds everything
from `jobs/` and treats in-flight presses as lost (one re-send).

## Waiting (the dont-poll rule made concrete)

```bash
scripts/emu/lab/lab.sh wait --job <id> [--max-time 3600]   # run_in_background; exits when the report exists
scripts/emu/lab/lab.sh wait --device any | --device <name>
scripts/emu/lab/lab.sh wait --queue-idle
```

`GET /wait?job=|device=|queue=idle&timeout=` is **one blocking GET**, held
up to `timeout` seconds (capped at 3600), answered `200` with the object when
the condition holds and `408 {timeout:true, state}` otherwise. The server
sends no keep-alive bytes (the body is JSON); `lab.sh` passes `--max-time`
a little past the timeout so the 408 is the server's. One background call
per wait; never a loop.

## The report (D5)

`report.mjs` computes it when the last press lands: per build × row key
(`render-basic/t2/jit/8`, `render-basic/t2/interp`), the values in press
order (`null` for excluded presses), `best` with its press, `median`,
`spreadPct = (max − min) / max × 100`; the **same-press ratio** translated ÷
interpreter per press (only when that press has an interpreter row for the
same slug/grade) with its best and median; byte-identity across the build's
rows (`uartSha256`; null = unknown, DD46); for an A/B, `best(B)/best(A) − 1`,
the same for medians, and for the ratio medians. Tainted and failed presses
are listed under `excluded` and never counted. `report.md` is the G-M7B
five-press table, generated.

## `lab.sh`

```bash
lab.sh status | devices | jobs | report <id> | cancel <id> | home | token | curl <path> [curl args]
lab.sh queue --build 23a3d3c --rows gate-rows --repeats 3 --spacing 3m [--device NAME] [--ttl 24h] [--note …]
lab.sh queue --ab 86cb2e0 23a3d3c --rows gate-rows --repeats 5 --spacing 3m
lab.sh queue --build X --row render-basic:t2:jit:8 --row render-basic:t2:interp
lab.sh wait --job <id> [--max-time 3600] | --device any|NAME | --queue-idle
```

`queue` prints the job id on stdout; `wait --job` prints the path of
`report.md`; `--spacing`/`--ttl` take `90s`, `3m`, `24h`.

## Verifying the page without opening it as a device

Agents never open a lab tab as a device (the headless rule; hidden tabs are
throttled and lie about timing). Three proofs, and only the third is a
number:

1. **Refusal** — the harness Browser pane is hidden, so a press sent to it is
   `deferred` and the board says so.
2. **Protocol** — `just test-emu-lab` drives the queue with a fake page
   client in-process.
3. **A real press** in headless Chrome (`--headless=new` reports `visible`;
   no wake lock, so rows carry `wakeLock: unsupported|denied`):
   ```bash
   /Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome --headless=new --remote-debugging-port=9333 \
     --user-data-dir=/tmp/lab-chrome 'http://127.0.0.1:41111/#t=<token>' &
   node scripts/emu/lab/test/cdp-join.mjs 9333 desk-chrome      # types the name, clicks Join over CDP
   scripts/emu/lab/lab.sh queue --build <id> --rows gate-rows --repeats 1 --spacing 0
   scripts/emu/lab/lab.sh wait --job <id>
   ```
   A desk-Chrome number is a V8 number under whatever load the desk is
   carrying; it proves the path, not the phone.

## Tests

```bash
just test-emu-lab        # node --test scripts/emu/lab/test/*.test.mjs
```

The tests spawn the real `server.mjs` on port 0 in a temp home
(`test/helpers.mjs`); `test/fake-device.mjs` is a page client that answers
presses from a table of numbers at test speed (`LAB_TICK_MS`,
`LAB_COOLDOWN_MS`, `LAB_LOST_MS`); `report.test.mjs` reproduces the G-M7B
five-press table exactly. Nothing here needs a package.json, and nothing
may gain one.
