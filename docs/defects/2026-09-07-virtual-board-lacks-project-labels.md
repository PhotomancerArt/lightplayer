---
status: open
found: 2026-09-07      # how: live-debugging
area: lp-fw/fw-emu boot manifest + lpc-hardware virtual_quad_rmt_gpio_board; catalog/ and projects/test/ output wires
class: stand-in-divergence
related:
  - 2026-09-02-ignored-emu-fuel-probe-renders-black-on-first-probe.md
  - 2026-09-04-read-gate-refuses-on-largest-block-proxy.md
  - "planning: 2026-09-07-0118-studio-emulated-boards (vision D30, D40-D42)"
---
# The emulator's stand-in board has none of the wire labels the catalog authors

**Symptom** — every WS281x output of `catalog/projects/zook-dome` fails to
open on the emulator, and no strip lights. From a fresh profile run on
`origin/main` (`4d4e58378`), with no board in the loop:

```
$ cargo run -q -p lp-cli -- profile catalog/projects/zook-dome --collect cpu,alloc --mode compile
...
[INFO  lpa_server::project_manager] Project loaded: Zook dome
[WARN  lpc_engine::engine::engine_services] EngineServices: output node 3 port 0 ws281x:local:IO18: Invalid config: unknown Ws281x hardware endpoint: ws281x:local:IO18
[WARN  lpc_engine::engine::engine_services] EngineServices: output node 3 port 1 ws281x:local:IO13: Invalid config: unknown Ws281x hardware endpoint: ws281x:local:IO13
[WARN  lpc_engine::engine::engine_services] EngineServices: output node 3 port 2 ws281x:local:IO2:  Invalid config: unknown Ws281x hardware endpoint: ws281x:local:IO2
[WARN  lpc_engine::engine::engine_services] EngineServices: output node 3 port 3 ws281x:local:IO14: Invalid config: unknown Ws281x hardware endpoint: ws281x:local:IO14
[WARN  lpc_engine::engine::engine_services] EngineServices: output node 3 port 4 ws281x:local:IO16: Invalid config: unknown Ws281x hardware endpoint: ws281x:local:IO16
[WARN  lpc_engine::engine::engine_services] EngineServices: 5 output wires failed this frame; reporting output node 3 port 0 ...
[WARN  lpa_server::server] LpServer::tick: Project Zook dome tick error: Core("output flush: output node 3 port 0 ws281x:local:IO18: ...")
```

The project loads, its shader compiles, frames advance, the profile
report is written — and the run is a measurement of an engine whose
every output is dark. Nothing in the exit code says so (`exit 0`); the
only signal is five warnings scrolling past inside the cycle counter.

**Root cause** — the emulator boots one hard-coded stand-in board, and
the wires the catalog authors are not on it.

`lp-fw/fw-emu/src/main.rs:110` boots
`HwManifest::virtual_quad_rmt_gpio_board()`
(`lp-core/lpc-hardware/src/manifest/hw_manifest.rs:91`), whose 256 GPIO
resources are labelled `D10` (gpio18), `D9` (gpio8), `D8` (gpio7),
`D7` (gpio44), and `GPIO<n>` for everything else. `zook-dome` addresses
`ws281x:local:IO18`/`IO13`/`IO2`/`IO14`/`IO16` — the DOM-Z-102's
silkscreen, from the board manifest checked in at
`lp-core/lpc-hardware/boards/domraem/dom-z-102.json`.

Endpoint resolution is exact string equality on `display_label`
(`VirtualWs281xDriver::gpio_for_endpoint`,
`lp-core/lpc-hardware/src/drivers/ws281x/virtual_ws281x_driver.rs:124`);
there is no numeric fallback, so `IO18` does not find gpio18 even though
gpio18 is right there wearing the name `D10`. The stand-in models the
*shape* of a board — 256 pins, four RMT timing channels — and diverges
in the one dimension the substitution never modelled: **what the pins
are called**, which is the only thing an authored output wire ever
names.

Each failed open pays for the miss twice: `open_ws281x_by_spec`
(`hw_system.rs:109`) calls `driver.endpoints()`, which materialises all
256 GPIO endpoints with a freshly computed status apiece, finds no
match, and only then returns `UnknownEndpoint` — five times per project
for zook.

The divergence is *specific to the emu*. The Desktop board manifest
(`lp-core/lpc-hardware/boards/lightplayer/desktop.json`, reached via
`default_desktop_hardware_manifest()`) was built deliberately so that
"every wire label the checked-in catalog authors (`IO*`, `D0`–`D13`,
`A01`–`A13`, `B01`–`B13`) resolves" — the sim is honest because its
board has room for everything. The emu is the runtime that did not get
that treatment.

**Scope** — labels the virtual board cannot resolve, against every
bundled project on `origin/main`:

| Content | Unresolvable wires | Labels |
| --- | --- | --- |
| `catalog/projects/zook-dome` | 5 of 5 | `IO13 IO14 IO16 IO18 IO2` |
| `catalog/projects/small-dome` | 26 of 26 | `A01`–`A13`, `B01`–`B13` |
| `catalog/projects/logo-sign` | 2 of 2 | `IO13 IO18` |
| `catalog/patterns/plasma-duo` | 1 of 2 | `D11` (the disc output on `D10` works) |
| `projects/test/zook-dome-1500` | 5 of 5 | `IO13 IO14 IO16 IO18 IO2` |
| `projects/test/penta-strands-v3` | 5 of 5 | `IO13 IO14 IO16 IO18 IO2` |
| `projects/test/quad-strips-v3`, `quad-gamma-v3`, `quad-gamma-full`, `quad60-v3`, `quad-equal100-v3`, `quad-wire-oracle` | 4 of 4 each | `IO14 IO16 IO18 IO2` |

Four of the fifteen catalog entries and eight of the test rigs are
affected; three catalog entries go entirely dark. Everything else
addresses `D7`–`D10`, which the stand-in has. Button and radio wires
(`button:local:D9`, `radio:local:0`) resolve throughout.

The `A*`/`B*` and `D11` rows are a *second* flavour of the same
divergence rather than the same one: those are Desktop-target wires, so
they resolve on the sim's manifest and would be wrong to expect from an
ESP-class emulated board at all. They are listed because today the
emulator presents its one stand-in regardless of what a project targets,
so on the emulator they fail identically.

**Fix** — none; deliberately not fixed here.

The obvious patch — teach the virtual board the `IO*` labels, or give
resolution a numeric fallback — would make zook light on the emulator
and would be the wrong change. Which board the emulator presents is not
a label-table detail; it is the open question in the studio emulated-boards
vision (`~/.photomancer/planning/lp2025/2026-09-07-0118-studio-emulated-boards/vision.md`),
whose ruled model says a project declares a **target** (a board catalog
id or Desktop) and a **device** runs it — real, emu, or sim — with the
emu being "the real firmware binary for the target's chip" and the sim
"wearing the target's manifest" (D40–D42, D30). Under that model this
defect dissolves: `zook-dome` targets the DOM-Z-102, and its emu boots
the DOM-Z-102 manifest, at which point `IO18` resolves because it is
that board's real silkscreen — no label table anywhere needs editing.
Widening the stand-in now would bake in the thing the vision is
removing: a runtime that is a board-shaped average instead of a board.

Two facts worth carrying into that work:

- **Nothing today declares a target.** No bundled project carries the
  `target` field (it lives on the package manifest and is advisory), so
  even a target-aware emulator has nothing to read for the catalog as it
  stands. Backfilling targets on catalog content is a prerequisite, not
  a follow-on.
- **`permissive_emu_hardware_manifest()` is not the answer either.** It
  wears the XIAO ESP32-C6's D-labels, so it resolves `D7`–`D10` and
  misses `IO*`, `D11`, `A*`, `B*` exactly as the quad-RMT stand-in does.

**Regression coverage** — none, and the gap is the interesting part.
There is no gate anywhere that asks "does every bundled project's output
wire resolve against the board the emulator boots?" The byte-gate over
`catalog/` and `projects/test` (`lp-cli` canonical-bytes walk) checks
that these files are canonical JSON, not that their contents can open.
The natural test is a host-side one over the checked-in manifests: for
each bundled project and each candidate board manifest, assert the
project's wire labels are a subset of that board's, which would both
catch this and give the target backfill something to be checked against.

**Lesson** — a stand-in earns the name by being substitutable *in the
dimension callers actually use*. This one was designed against the
dimensions the tests exercised — pin count, RMT channel count, claim
contention — all of which are about capacity, and it is faithful in
every one of them. What authored content uses is none of those: it uses
the pin's **name**, which is the one attribute a synthetic board has no
reason to get right and every reason to get wrong. The tell is that the
failure is total and silent: not "three of five strips lit" (a capacity
symptom, which the stand-in was built to reproduce) but zero of five,
under an exit code of 0, in a profile report that still prints. When a
stand-in's divergence sits in an identity attribute rather than a
capacity one, the symptom stops looking like degradation and starts
looking like nothing at all. The general rule: when a fake stands in for
a member of a *catalog* of real things, prefer booting a real member of
that catalog over synthesising an average of them — the average is a
member of the catalog that does not exist, and content is authored
against members that do.
