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

## Tests

```bash
just test-emu-lab        # node --test scripts/emu/lab/test/*.test.mjs
```

The tests spawn the real `server.mjs` on port 0 in a temp home. Nothing here
needs a package.json, and nothing may gain one.
