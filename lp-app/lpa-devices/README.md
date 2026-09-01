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
| `replay.rs` | The fixture harness |

Tests live at the bottom of each file (repo convention); the scenario
suite and the property tests are `tests/scenarios.rs` and
`tests/properties.rs`, which drive the public surface only.

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
