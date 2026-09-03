# `lpa-devices` — the device model

The device layer's model: a roster of devices built from an event fold,
with no UI and no IO. Studio, `lp-cli`, and the tests all drive the same
object.

This crate exists because the shipped device layer was not one broken
thing but four state machines and five auxiliary stores with no shared
lifetime, and every recurring bug (two cards for one board, a vanished
Danger tab, a stale verdict, a connect that could not be cancelled) was an
identity-join or lifetime failure between them. See
`docs/adr/2026-08-25-event-fold-device-model.md` for the decision and
`~/.photomancer/planning/lp2025/2026-08-24-1612-device-serial-reliability/vision.md`
for the ratified vision.

## The shape

```text
Roster ──── owns ────► links (dumb transports) + the router
   │                   DeviceRecords (persisted identity + prefs)
   │                   Journal (flight recorder, both streams)
   └── owns ────► Device (one per known device)
                     │  intent      (prescriptive user state)
                     │  evidence    (incremental fold of events)
                     │  link        (routed by the roster)
                     └─ activity    (Option — supervised reducer)
                            projection: view DTO = f(intent, evidence, activity)
```

One entry point: `Roster::handle(now, input) -> Vec<Command>`. Two arms
with different rights:

| Arm | May write | May not write |
|---|---|---|
| `Input::Action` (user gesture) | `intent`, activity existence | `evidence` |
| `Input::Event` (the world) | `evidence` (via the fold), the running activity | `intent` |

Everything the model wants done leaves as a `Command`: a link command, a
timer, a record write, a grant revocation. That list is the complete set
of side effects the device layer can cause.

## The discipline rules

**Fold discipline (invariant I6).** New facts enter as events or they do
not enter. `Evidence` is written only inside `Evidence::fold`; a `bool`
grown beside the fold is the bug this crate exists to prevent. There is a
test for it: `device::tests::actions_never_touch_evidence` runs every
device-level action against real evidence and asserts nothing moved.

**No ambient time.** Every entry point takes a caller-supplied `Millis`
(epoch milliseconds — integers, so fixtures are exact and journals are
byte-reproducible). Waiting is `Command::StartTimer` out,
`Event::TimerFired` in. Each scope keeps **one** armed timer, generation-
stamped, so a superseded fire is dropped instead of churning the timeline
— which is why the command vocabulary needs no `CancelTimer`.

**Forbidden dependencies.** `tokio`, `embassy`, `wasm-bindgen`, `dioxus`,
`futures` executors — any of them in `Cargo.toml` means the model has
started doing IO. The crate compiles for `wasm32-unknown-unknown` and the
host, and the only dependencies are `serde` and `serde_json`. If something
seems to require an executor, the effects layer is the place for it.

**Dependency inversion.** The transport contract (`Link`, `LinkEvent`,
`LinkCommand`, `ResetKind`) is defined *here*; `lpa-link` implements it.
The model never calls a transport, and no transport classifies a device —
the hello gate, boot-line diagnosis and foreign-firmware detection all
live in the fold, which is what makes verdicts non-sticky.

**Non-sticky verdicts.** `Classification` is not a transition target; it
is recomputed from the current observation window on every fold. Opening a
port, a successful reset, and a detach all clear the window. A board that
boots noisily and *then* hellos is a LightPlayer, not a permanently blank
chip, and a device that is not plugged in is classified as nothing at all.

**Bounded cancellation.** Cancel is *requested*: the activity gets a
grace period to wind down (Identify hands the port back and waits for the
close), and then it is **evicted** — journaled, link rebuilt, evidence
re-derived. Bounded by removal, not by politeness.

**Total, escapable projection.** `view::roster_view` renders every
reachable state, and every card carries at least one `Escape`. `Forget` is
defined at the model level, so it cannot be conditioned away — including
for an anonymous board mid-activity, which the shipped system could never
forget.

## The fixture-replay pattern

A fixture is a timestamped, interleaved script of actions and events with
projection assertions at marked steps. The runner owns a virtual clock:
timers the model asks for are scheduled, and when the script advances,
every due timer fires in order before the next scripted input.

```rust
use lpa_devices::replay::{Expect, Replay, Script, Step};
use lpa_devices::RosterConfig;

let fixture = Script::new()
    .at(0, Step::attach(1, "usb-1"))
    .expect(Expect::new().pending(1).devices(0))
    .at(20, Step::opened(1))
    // A RUNNING server heartbeats; the hello ANSWER comes later.
    .at(500, Step::heartbeat(1))
    .expect(Expect::new().journal_notes_absent(&["ActivityEnded"]))
    .at(1_500, Step::hello(1).uid("dev_2f8a").board("dig-uno"))
    .expect(
        Expect::new()
            .devices(1)
            .device_state("Ready")
            .outcome_contains("dig-uno"),
    )
    .into_fixture("mid-stream attach");

let mut replay = Replay::new(RosterConfig::default());
replay.run(&fixture).expect("scenario");
```

The same thing as JSON lives in `fixtures/`, parsed by
`Fixture::from_json` and loaded with `include_str!` (no file IO in tests):

```json
{
  "at_ms": 1500,
  "do": { "hello": { "link": 1, "uid": "dev_2f8a", "board": "dig-uno" } },
  "expect": { "devices": 1, "device_state": "Ready", "escapes": ["Disconnect", "Forget"] }
}
```

Use a JSON fixture when the scenario is a *trace* worth reading on its own
(a bug timeline, a golden capture from M8); use a `Script` when it is a
programmatic sweep. Both go through the same runner.

## Layout

| File | Concept |
|---|---|
| `roster.rs` | Routing, records, pending links, `Forget`, merges, `RosterConfig` |
| `device.rs` | `Device::handle`, supervision (cancel → grace → evict → recover) |
| `evidence.rs` | The fold: presence, classification, freshness, observations |
| `intent.rs` | The prescriptive half |
| `activity/` | Supervision machinery + the `Identify` reducer |
| `identity.rs` | The bindings chain: endpoint → MAC → uid → name |
| `journal.rs` | Flight recorder: both streams, derived notes, ring pruning |
| `view.rs` | The projection and the escape invariant |
| `link.rs` | The transport contract `lpa-link` implements |
| `wire.rs` | Minimal mirror of the wire facts the fold reads |
| `record.rs` | Persisted identity + prefs snapshot (name, autoconnect, last seen, board id, chip) |
| `replay.rs` | The fixture harness |

Tests live at the bottom of each file (repo convention); the scenario
suite and the property tests are `tests/scenarios.rs` and
`tests/properties.rs`, which drive the public surface only.

## The terminal panel

`Evidence` keeps a typed log of everything worth showing on a card's
terminal — raw serial lines, decoded wire frames and Studio's own activity
narration, in the order they happened, surviving the reopen a flash's
reconnect ladder performs (a window reset restarts *classification*, never
the log). `DeviceView.terminal: Vec<TerminalLine>` is the projection of it:

```rust
pub struct TerminalLine { pub kind: TerminalKind, pub text: String, pub repeats: u32 }

pub enum TerminalKind { Rom, Board, Wire, Studio, Outcome, Failure, Recovery }
```

- **Kind** says who produced the line, for the renderer's colour: `Rom` is
  the ROM bootloader's own chatter (`ESP-ROM:`, `rst:`, `boot:`, an
  `invalid header` boot loop, …, mirroring `chip_from_boot_line`'s
  signatures); `Board` is anything else the running server printed;
  `Recovery` is a `[RECOVERY]`-prefixed line from the crash ledger; `Wire`
  is a decoded frame (below); `Studio` is the model's own narration
  (an activity starting, or its progress label — no more `"— … —"`
  dressing, since the kind now carries that); `Outcome`/`Failure` is how an
  activity ended.
- **Cap and drop.** The log is bounded at 200 lines; past that the oldest
  is dropped and counted in `Evidence::terminal_dropped` /
  `DeviceView.terminal_dropped`, which is never reset (`output` itself
  survives a window reset for the same reason — see its doc — so a counter
  that reset with the window would undercount right after the reconnect
  ladder that made it matter).
- **Repeat collapse.** `push_output` collapses a consecutive identical
  `(kind, text)` pair into the previous line's `repeats` count instead of
  dropping it — this is what keeps a percent-ticking flash label or an
  unchanging heartbeat from filling the panel with two hundred copies of
  the same line.
- **Wire frames reach the panel.** `LinkEvent::Frame` used not to be pushed
  at all, which is why a heartbeat was invisible even though it is the
  clearest proof of life a board sends. Every frame decodes to one line via
  `TerminalKind::Wire`:
  - `Hello` → `hello · proto {n} · {board_id or "?"} · {firmware or "?"}`.
  - `Heartbeat` → `heartbeat · {project label|idle}`, plus
    ` · FAULT {label}` when the first loaded project carries a fault or the
    recovery facts are degraded. Deliberately excludes uptime, a frame
    counter, or any other fact that changes on every healthy tick — this
    mirror crate carries no fps/heap facts on `LoadedProjectFacts` or
    `RecoveryFacts` today (see `wire.rs`'s module doc on why the mirror
    stays small), and even if it did, anything that always changes would
    turn every heartbeat into a new line instead of one collapsing with a
    repeat count.
  - `Loaded` → `loaded · {n} project(s)`, names joined by `, `.
  - `Other` → the label, verbatim.

## Board id and chip: learned, never cleared

`DeviceRecord` gains `board_id: Option<String>` and `chip: Option<String>`
(both `#[serde(default)]`, so a registry row written before they existed
still loads). `Device::record_snapshot` learns `board_id` from a settled
hello's `HelloFacts::board_id` and `chip` from `Evidence::detected_chip`
(the boot-banner reader) — the same shape `last_seen` already uses: a new
`Some` overwrites, a `None` leaves the stored value alone. This matters
because a window reset (a reopen, a reboot) clears the *current*
observation window's hello and boot banner — that is what "non-sticky
verdicts" means — but the record's job is to remember what was already
learned, not to re-derive it every time. `DeviceView.board_id` and
`DeviceView.detected_chip` fall back to the record when the current window
has nothing of its own to say, so an attached-but-closed or freshly
rehydrated board keeps the identity line it already earned. The chip
*family* join (a board id or a firmware package name → a catalog entry) is
an app-layer concern, above this crate — see `docs/adr/2026-08-25-event-fold-device-model.md`
and the device-card-v2 plan's D2.

`DeviceView.firmware` carries the settled hello's raw firmware label
verbatim (`HelloFacts::firmware`), `None` on any board that has not said
hello this window. Unlike `board_id`/`chip` it is NOT record-backed — a
window reset drops it, honestly, since firmware is a live report rather
than a durable identity fact. The app-layer identity line (P2,
`lpa-studio-core::device_identity_line`) reads it to render "fw …" or "no
firmware".

## Validation

```bash
cargo test -p lpa-devices
cargo check -p lpa-devices --target wasm32-unknown-unknown   # `just check` skips wasm32
```

## Not here yet

- The Pull activity (round-2 M4) — one more `ActivityKind` variant, one
  more `Reducer` arm, one more `EffectRequest` arm, following the shape
  Flash (M2) and Push (M3) share. The old Setup/Provision orchestrators
  dissolved into the card ruling: the card face picks the verb from fold
  evidence.
- Sync verdicts and the banking verbs (M4). What a board is running enters
  as evidence here (`Evidence::loaded_projects`, off the heartbeat's report
  and the `ListLoadedProjects` answer); relating that to a library project
  is a projection-time JOIN in the app, and `DeviceSyncState` is NOT coming
  back as a store.
- The sim as a peer `Link` implementation (vision R1, slice S5).
