# Device setup flow

Status: implemented as a pure reducer (P11, 2026-08-05). Graduated from the
gallery-product-vision plan's `flow-spec.md` after its G1 ruling.

**This document and `lp-app/lpa-studio-core/src/app/setup_flow/` are one
artifact.** The §2 transition table below is the contract the reducer's
match arms implement and the transition tests enforce. A change to either
lands in the SAME commit as the change to the other — a doc that describes a
machine the code does not implement is worse than no doc.

The steps are **Connect → Flash → Provision → Device home**, with
board-state detection inside Flash, naming derived at Provision, and full
simulator variants. The flow is testable end to end without hardware.

## 1 · Context: what already exists (build on, don't fork)

| Piece | Where | Role in this flow |
|---|---|---|
| Web Serial port grant + link session | `lpa-link` providers, browser | Connect step's port picker |
| Chip probe evidence (`detected_chip`) | `lpa_link::DeviceSnapshot` | filters the board pick |
| Probed base MAC (`probed_mac`) | `lpa_link::DeviceSnapshot` | the identity anchor (A2) |
| Identity resolution | `app/places/identity_resolution.rs` | `IdentityEvidence` → `ResolvedIdentity` |
| Boot-line diagnosis | `lpa_link::BootLineClassifier` | the blank / ROM-download signatures §4 reads |
| Flash activity view (progress/steps/log) | card-owned op flows | Flash step's working state, reused verbatim |
| Registry (`RegisteredDevice`) | `app/places/device_registry.rs` | written at Provision; adopt reads it |
| Card-owned op flows, `RosterCardState` | device lifecycle (#140) | the wizard drives the same ops |
| First-project generator | `app/home/board_project.rs` (`CatalogOp::GenerateForBoard`) | Provision's project |
| The single sim session (`runtime-sim`) | studio controller | the no-connect, no-flash target |

## 2 · The state machine

One machine, one entry point parameterised by the target's **capabilities**
(§6 R2), not by "is it the simulator". Notation: `STATE —event→ STATE`.
Every transition below is a required unit test; every pair NOT below is
required to be inert (state unchanged, no commands).

`SetupFlow::start` opens on `CONNECT_INTRO` when the target
`needs_connect`, and on `BOARD_PICK` when it does not (the simulator's
entry — flow-spec called that state `SIM_BOARD_PICK`; see §7).

```text
CONNECT_INTRO                    "connect your device now" — two stacked full-width CTAs
  —ItsConnected→                 PORT_PICKING          [RequestPort]
  —PickBoardFirst→               BOARD_FIRST
                                 (cancel hints escalate toward the secondary CTA)
BOARD_FIRST                      full-catalog board pick, then board-specific connect
                                 guidance — driver steps for CH340-class boards,
                                 cable/port tips otherwise
  —BoardChosen→                  BOARD_FIRST           (records the choice; pre-seeds BOARD_PICK)
  —ItsPluggedIn→                 PORT_PICKING          [RequestPort]
  —Back→                         CONNECT_INTRO
PORT_PICKING                     the browser owns the chooser dialog
  —PortChosen→                   CONNECTING            (the chooser answered; the connect
                                 it started is already running, so this asks for nothing)
  —PortGranted→                  PROBING               [ProbeBoard]
                                 (a grant with no CONNECTING in between: the port was
                                  already open, or a caller that does not split the phases)
  —PortPickerCancelled→          CONNECT_INTRO         (hint: picker closed)
  —PortPickerEmpty→              CONNECT_INTRO         (hint escalates to BOARD_FIRST)
CONNECTING                       opening the port, resetting, waiting for the hello —
                                 the SECONDS that used to wear PORT_PICKING's copy
  —PortGranted→                  PROBING               [ProbeBoard]
  —PortPickerCancelled→          CONNECT_INTRO         (the connect ended with no session)
PROBING                          chip probe + board-state detection, one spinner
  —ProbeCompleted(Blank)→        BOARD_PICK            (chip known → catalog filtered + Generic)
  —ProbeCompleted(Wled)→         WLED_FOUND
  —ProbeCompleted(LightPlayer)→  ALREADY_LP
  —ProbeCompleted(StaleLightPlayer)→ STALE_LP
  —ProbeCompleted(Unresponsive)→ PROBE_FAILED
  —PortLost→                     CONNECT_INTRO         (hint: device disconnected)
BOARD_PICK                       picker filtered to the detected chip + Generic;
                                 pre-selected from BOARD_FIRST when chip-compatible;
                                 carries recognition ("was Porch sign") when the probed
                                 MAC matches a registry row
  —BoardChosen→                  BOARD_PICK            (records the selection)
  —Confirm→                      FLASHING              [Flash]      when needs_flash
  —Confirm→                      PROVISION                          when not needs_flash
  —Confirm (nothing picked)→     BOARD_PICK            (the forward verb is not armed)
  —Back→                         CONNECT_INTRO         [ReleasePort]   when needs_connect
  —Back→                         CLOSED(Cancelled)                     otherwise (it is the entry state)
  —SetUpElsewhere(board)→        CLOSED(SetUpElsewhere)                when !needs_flash (§7.14)
  —SetUpElsewhere(none)→         BOARD_PICK            (nothing inferred; the picker still asks)
  —SetUpElsewhere→               BOARD_PICK            (inert when needs_flash — no firmware yet)
PROBE_FAILED                     retry / replug hint / BOARD_FIRST link
  —Retry→                        PROBING               [ProbeBoard]
  —PickBoardFirst→               BOARD_FIRST           [ReleasePort]
  —Back→                         CONNECT_INTRO         [ReleasePort]
WLED_FOUND                       "This board runs WLED." (may carry recognition)
  —WipeAndSetUp→                 BOARD_PICK            (flash replaces WLED; migration is future work)
  —Back→                         CONNECT_INTRO         [ReleasePort]
ALREADY_LP                       "Already running LightPlayer" + identity card
  —AdoptDone→                    CLOSED(Adopted)       [RecordSighting?]
                                 (primary "Done": adopt and STAY — no lens attach, and no
                                  ReleasePort either; the board keeps its session and its card)
  —AdoptAndOpen→                 DEVICE_HOME           [RecordSighting?, OpenDeviceHome]
                                 (secondary "Open in the editor →" — what Done used to do)
  —SetUpFresh→                   BOARD_PICK            (re-flash path; warns before writing)
STALE_LP                         "Running an older LightPlayer" + identity card
  —UpdateFirmware→               BOARD_PICK            (the update IS a flash; warns before writing)
  —Back→                         CONNECT_INTRO         [ReleasePort]
FLASHING                         activity view + step checklist
  —FlashSucceeded→               PROVISION
  —FlashFailed→                  FLASH_FAILED
  —PortLost→                     FLASH_FAILED          (the link layer surfaces it as a flash failure)
  —CloseRequested→               ABANDON_GUARD
FLASH_FAILED                     ✗ on checklist, log tail kept
  —Retry (1st)→                  FLASHING              [Flash{attempt:2, replug guidance}]
  —Retry (2nd+)→                 FLASHING              [Flash{attempt:n, replug guidance}]
  —Abandon | CloseRequested→     CLOSED(IncompleteFlash) [MarkIncompleteFlash, ReleasePort]
  —PortLost→                     FLASH_FAILED          (inert: the replug the guidance asked for)
ABANDON_GUARD
  —KeepFlashing→                 FLASHING              (resumes; the flash never actually paused)
  —Abandon→                      CLOSED(IncompleteFlash) [MarkIncompleteFlash, ReleasePort]
  —FlashSucceeded→               PROVISION             (the flash never paused; the sheet is moot)
  —FlashFailed→                  FLASH_FAILED
  —PortLost→                     FLASH_FAILED
  —CloseRequested→               ABANDON_GUARD         (inert: the sheet IS the answer to ✕)
PROVISION                        see §3
  —NameEdited→                   PROVISION             (edits the derived name; inert when !can_rename)
  —Confirm→                      PROVISION             [GenerateProject]   (phase: Generating)
  —ProjectGenerated→             PROVISION             [WriteRegistry?, PushProject] (phase: Pushing)
  —PushCompleted→                DEVICE_HOME           [OpenDeviceHome]
                                 (each of the three is inert out of phase — one click, one generate)
  —CloseRequested→               CLOSED(LeftConnected)               when needs_connect
                                 (the flash ALREADY LANDED: no ReleasePort, nothing marked —
                                  the board is alive and stays on the roster with its port)
  —CloseRequested→               CLOSED(Cancelled)                   otherwise (the sim holds no port)
DEVICE_HOME                      the editor lensed to the device, project running (§5)
CLOSED                           terminal
```

### Cross-cutting

- `CloseRequested` anywhere outside FLASHING / FLASH_FAILED /
  ABANDON_GUARD / PROVISION → `CLOSED(Cancelled)`, no guard, state
  discarded (nothing was written before FLASHING; adopt writes nothing).
  It releases the port when one was granted. **PROVISION is the
  exception**: by then the flash has landed, so ✕ there is
  `CLOSED(LeftConnected)` and the port is KEPT — see §7.12. DEVICE_HOME
  and CLOSED ignore it — the card owns the surface from there.
- `PortLost` in any hardware state that holds a port (PROBING through
  PROVISION) → CONNECT_INTRO with a hint; during FLASHING or ABANDON_GUARD
  it presents as a flash failure; at FLASH_FAILED it is inert.
- Every `(State, Event)` pair not listed in §2 is **inert**: the state comes
  back unchanged and nothing is asked for. A stale click or a late event
  from a superseded step is not an error, and panicking on the user would
  be worse. The transition test asserts inertness pair by pair rather than
  assuming it.
- Wizard state is not persisted across refresh (alpha posture); a refresh
  mid-flash lands on the incomplete-flash card state.
- `WriteRegistry` is emitted whenever the target `can_rename` — the
  simulator names nothing (§3). Its `hardware_uid` is **advisory**: the
  uid the PROBE anchored, when it anchored one. A blank board probed in
  its boot loop anchors nothing, and the flash in between is exactly what
  gives it an identity, so the EXECUTOR addresses the row with the bound
  session's currently resolved uid and falls back to the probe's (§8).
  Neither available = no row, said out loud. `RecordSighting` still needs
  a probed uid: a board anchored to nothing is adoptable, it is just not
  remembered.
- **CLOSED carries a reason**: `Cancelled` (nothing was written),
  `IncompleteFlash` (the board's card is marked — it needs re-flashing),
  `Adopted` (§5), `LeftConnected` (§7.12 — ✕ after the flash landed), and
  `SetUpElsewhere` (§7.14 — a project landed on the target while the
  picker was still asking). Only the first two release the port: a board
  that is flashed, adopted, or both has earned its place on the roster,
  and dropping its session on the way out is how it ends up reading "not
  connected" one frame after it was set up.
- **`SetUpElsewhere` is an OUTCOME, not a gesture**, like `ProbeCompleted`
  and `FlashSucceeded`: no component can name it, only the controller
  reporting what the world did. `gesture.rs`'s partition test enforces it.

## 3 · Provision step (shared)

**New project only.** Load-existing is deferred; compose/enhance later. The
step is one box + one field:

- **Your first project — made for the \<board\>**: the generated package
  (`generate_board_project`) — clock → playlist(meteor) → fixture → output
  on the board's first default LED wire, manifest `target` set to the board.
- *Future work*: load-existing pane + target-mismatch warning.

**Naming: here, derived, rarely typed.** One editable field, prefilled:

| Situation | Default device name |
|---|---|
| Known board (registry row exists) | remembered registry name |
| New generated project | `<board label> · <Mon D>` |
| Collision with an existing device name | ` 2`, ` 3`, … suffix |

The generated **project** name keeps the library convention
(`YYYY-MM-DD-HHMM-<board-slug>`) — that is `LibraryStore::install_package`'s
existing `dated_slug` behaviour, not something this flow re-implements.

Sim path: no name field (`can_rename` is false) — the single sim stays
"Simulator". Hardware: the name is written to the registry under the probed
`HardwareId` at PROVISION. **There is no stamp step**: identity is anchored
in silicon and an erase keeps it (`docs/adr/2026-08-04-device-identity-anchored-in-silicon.md`).

## 4 · Board-state detection (Flash step, hardware)

One probe pass, one verdict — a first-class enum, not scattered ifs:

| Verdict | Evidence | Flow |
|---|---|---|
| `LightPlayer { known }` | a proto-matching `ServerHello` arrived — **and nothing else** | ALREADY_LP |
| `StaleLightPlayer { known }` | the link classified the peer `DeviceState::Incompatible` — LightPlayer framing, no proto-matching hello | STALE_LP |
| `Wled { known }` | a serial/Improv line names WLED | WLED_FOUND |
| `Blank { known }` | a no-firmware boot signature (`invalid header: 0xffffffff`, ROM download mode, a known replaceable banner) | BOARD_PICK |
| `Unresponsive { known }` | nothing intelligible | PROBE_FAILED |

`known: Option<RegisteredDevice>` comes from one registry lookup keyed by
the probed MAC's derived uid. `Unresponsive` carries the field for
uniformity; in practice a board that said nothing intelligible has no MAC
either, so it is `None`.

**The classification bias is asymmetric and deliberate.** Under-claiming is
cheap: a WLED board classified `Blank` only means the wipe offer is skipped,
and the flash confirmation still guards the data. Over-claiming
`LightPlayer` is not recoverable by any later guard — it routes a stranger's
board into ALREADY_LP and offers to adopt it. So the hello is the ONLY
evidence that yields `LightPlayer`, and WLED detection is deliberately
conservative (a banner match; ambiguous Improv traffic alone is not enough).

`StaleLightPlayer` is the one verdict that reads LightPlayer-ish and still
must not be `LightPlayer`: the board speaks our framing but offers no
protocol this Studio can use, so there is nothing to adopt. It outranks the
WLED banner and the no-firmware signature because it names the firmware
exactly, and it lands on STALE_LP — whose one affordance is the reflash the
link layer already prescribes (`DeviceState::Incompatible`). Before it
existed, a board on old firmware fell through to `Unresponsive` and was told
to hold BOOT, which fixes nothing.

## 5 · Device home

The editor, lensed to the device — project tree + lensed node + the device
card grown as the right pane, LEDs already animating. It is where the SETUP
path lands (PROVISION → PushCompleted): a board that had nothing on it now
has a project, and the editor is where that project lives.

**Adopt does not land there** (G2 follow-up, 2026-08-05). ALREADY_LP's
"Done" ends at `CLOSED(Adopted)`: the sighting is recorded, the port is
kept, the card returns to its own body on the roster, and the user stays on
the gallery. Adopting a board that was already glowing is not a setup —
nothing was created and nothing needs opening — and being thrown into the
editor for it read as one. The board's existing project is one click away
on its card, and the state keeps a secondary verb, "Open in the editor →"
(`AdoptAndOpen`), for whoever wanted exactly that.

## 6 · Architecture

- **R1 — `SetupFlow` is a pure reducer**: `(State, Event) → (State,
  Vec<Command>)`. No I/O, no async, no UI types. The UI renders `State`; an
  executor runs `Command`s. Every transition above is a table-driven unit
  test with zero I/O.
- **R2 — one `SetupTarget` capability boundary for hardware and sim**: the
  machine branches on `needs_connect` / `needs_flash` / `can_rename`, never
  on "is sim". The M9 wasm fake at the Web Serial boundary therefore drives
  the FULL hardware path in integration tests, flash included.
- **R3 — the board-state verdict enum** (§4) replaces implicit "did hello
  answer" checks at this seam.
- **R4 — the naming derivation helper** (§3) is shared by the wizard and the
  card rename placeholder.
- **Deliberately untouched**: the flash implementation and the card-owned op
  flows. They encode the espflash / S3-pty / #292 lessons; the flow
  composes them through `SetupDispatch` (§8).

## 7 · Where this differs from the ratified flow-spec

Recorded so the two can be reconciled rather than silently diverge:

1. **`SIM_BOARD_PICK` is not a separate state.** It is `BOARD_PICK` entered
   with no probe evidence. Keeping two states would have been a branch on
   "is sim" in disguise, which R2 forbids; the outgoing edge is chosen by
   `needs_flash` instead. The steps rail (`Board › Project › Done`) stays a
   UI concern.
2. **`BOARD_PICK` separates selection from confirmation.** flow-spec's
   "board chosen, Flash" is `BoardChosen` (records) then `Confirm`
   (advances). The sim path sends both; the hardware path's `Confirm` is
   the Flash button.
3. **PROVISION is internally sequenced.** flow-spec's single edge "project
   ready, pushed → DEVICE_HOME" is the last of four: `Confirm` →
   `ProjectGenerated` → `PushCompleted`. The intermediate edges keep the
   state and advance a phase, so the F2 edge itself is unchanged.
4. **`PROBE_FAILED` gained explicit `Retry` / `PickBoardFirst` / `Back`
   edges.** flow-spec described them parenthetically ("retry / replug hint
   / BOARD_FIRST link") without naming events.
5. **`Unresponsive` carries `known`** (see §4).
6. **The board label in the derived name is the catalog `display_name`
   verbatim** ("XIAO ESP32-C6 · Aug 4"), not a shortened form. flow-spec's
   "C6 DevKit" was illustrative; no catalog field carries a short label, and
   deriving one by string surgery produced worse results than the honest
   name. If the catalog ever gains a short label, the helper reads it.
7. **Closing on FLASH_FAILED is the same act as abandoning.** flow-spec
   gave FLASH_FAILED only an `abandon` edge and sent ✕ everywhere else to a
   clean CLOSED. A board with a part-written flash cannot be left un-marked
   just because the user reached for the ✕ instead of the button.
8. **ABANDON_GUARD handles the flash landing under it.** The guard is a
   sheet over an operation that never stopped, so `FlashSucceeded` /
   `FlashFailed` / `PortLost` are handled there exactly as in FLASHING.
   Without those arms an ✕ pressed a moment before the flash finished would
   strand the flow. ✕ pressed again while the sheet is open is inert.
9. **`PortLost` at FLASH_FAILED is inert.** The literal "≥ PROBING → back
   to CONNECT_INTRO" rule would throw away the retry affordance on exactly
   the boards that need it: the second-attempt guidance ASKS for a replug,
   and a replug is a port loss.
10. **No PROVISION failure edge exists yet — still open after P06.** A
    generate or push failure has nowhere to land but `CloseRequested`. The
    UI phase (P06) did NOT close it: the missing decision is a product one
    — what a failed push leaves on a board that is already flashed and
    written to the registry — and inventing a transition to render would
    have been answering it by accident. What P06 does instead is refuse to
    hide it: the controller records the error on the wizard card
    (`UiSetupWizard::error`, outside the machine), the PROVISION step shows
    it, and the ✕ stays the door. The board keeps whatever landed on it and
    appears on the roster, where a project can be pushed from the gallery.
    **The cost of leaving it open**: the forward verb stays disabled in the
    `Generating`/`Pushing` phase, so a failure cannot be retried in place —
    the user has to close and start again. Closing the gap means naming the
    edge (`GenerateFailed` / `PushFailed` → PROVISION at
    `ProvisionPhase::Editing`, presumably) in §2, the reducer, and the
    transition tests together.
11. **Adopt has two edges, and the primary one does not navigate.**
    flow-spec (and P11) gave ALREADY_LP a single `AdoptDone` →
    DEVICE_HOME. The G2 re-walk (2026-08-05) rejected the landing: "Done
    takes you right into the editor. That doesn't feel right… Done should
    just go to the main state." So `AdoptDone` now ends at
    `CLOSED(Adopted)` — sighting recorded, port kept, no lens attach — and
    the old behaviour became the secondary `AdoptAndOpen` ("Open in the
    editor →"). The full setup path is untouched: creating a project and
    then landing in the editor with it running is the ratified north star,
    and the difference is exactly that adopt creates nothing.
12. **✕ at PROVISION keeps the port.** flow-spec's close rule ("nothing
    was written before FLASHING, so ✕ releases and discards") is right up
    to the flash and wrong after it. The G2 walk (2026-08-05) closed the
    wizard at PROVISION on a board that had JUST been flashed
    successfully, and the board went straight to "not connected" — with a
    Reconnect that then had to re-open a port the app had released a
    second earlier. The flash landed; the board is alive and running our
    firmware; it belongs on the roster. So PROVISION's ✕ is
    `CLOSED(LeftConnected)` with no commands at all, and the card simply
    drops the wizard's body for its own (usually Connected-empty: flashed,
    nothing on it yet). The simulator, which holds no port, keeps the
    plain `CLOSED(Cancelled)`.
13. **The provisioning name is written under the identity the FLASH gave
    the board.** `WriteRegistry` used to be emitted only when the PROBE
    had anchored a uid. A blank board probed in its boot loop anchors
    none — so on the G2 walk the name the user typed was written nowhere,
    and the push one command later refused the board with "no named
    device is connected" (the push gate reads the session identity's
    NAME). The reducer now always asks for the write and passes the
    probe's uid as advisory; the executor, which can see the live
    session, addresses the row with whatever identity the board actually
    has by then. Two follow-on repairs live in the controller, not the
    machine: the provision commands wait for the board to be back
    (`wait_ready`, idempotent and bounded by the link layer's own
    deadline) before writing or pushing, and the name is applied to the
    session's cached identity directly — because the post-write refresh
    CLEARS the reconcile state before re-reading, so a re-read that
    cannot run would otherwise leave the board with no identity at all,
    one command before the push demands one.

14. **A target can be set up out from under the flow.** flow-spec assumed
    the wizard was the only way a target gets set up, and BOARD_PICK is
    where that assumption bites: "Open in sim" on a project card loads
    that project onto the simulator, and the project's advisory manifest
    `target` becomes the sim's board on the way in (honest-device-preview
    vision D4). The wizard went on asking for a board the landing had
    already supplied (G1b ruling 6, 2026-08-05).

    `SetUpElsewhere { board_id }` is that report. It **closes** the flow
    rather than walking it on to PROVISION, because PROVISION *generates
    a starter project and pushes it* — here that would overwrite the
    project the user just opened — while everything setup exists to
    produce (a running target, a board, a project) is already true. So
    CLOSED is the completed outcome, not a shortcut past one.

    Two capability guards, no kind check (R2): a target that still
    `needs_flash` is not set up by a project landing somewhere else (it
    has no firmware yet), and `board_id: None` — the untargeted project —
    infers nothing, so the picker stays the only way to answer. The
    controller emits it only for a sim flow whose lens is the sim, so a
    hardware wizard never sees it either.

    **The landing has two shapes, and both report it.** The push
    (`open_pending_package`) is the obvious one. The other is the D37
    re-attach: when the sim is ALREADY running the requested project,
    "Open in sim" re-attaches the lens instead of pushing a head, so
    nothing loads and `note_sim_loaded_project` never runs — the project
    landed on an earlier click and the board came with it then. Found on
    the G1 dev-server walk (2026-08-06) with the push path already fixed;
    the picker stayed up on a sim that had been wearing its board the
    whole time.

## 8 · Command → existing machinery

The executor is a pure mapping (`setup_flow/executor.rs`); it decides
nothing. Each `SetupCommand` names machinery that already exists:

| Command | Existing machinery |
|---|---|
| `RequestPort` | `DeviceOp::OpenProvider { BrowserSerialEsp32 }` — run as its two phases (`DeviceController::choose_provider_endpoint`, then `connect_endpoint`) so the executor can report `PortChosen` between them; that interim event is the one thing a command reports before it finishes, and it is state-only by construction (there is no queue to run commands on from inside a command) |
| `ProbeBoard` | read `DeviceSession::snapshot()` (`detected_chip`, `probed_mac`, `recent_lines`) → `classify_board`; escalate with `DeviceOp::ProbeBootloaderMode` only when the passive read is `Unresponsive` |
| `ReleasePort` | `DeviceOp::DisconnectDevice { target }` |
| `Flash` | `DeviceOp::ProvisionFirmware { target, setup_name: None, board_id }` — `setup_name` is `None` because naming is a Provision-time registry write |
| `GenerateProject` | `CatalogOp::GenerateForBoard { board_id }` |
| `WriteRegistry` | `CatalogOp::UpsertRegisteredDevice(..)` (merge upsert), addressed by the bound session's resolved uid, else the command's advisory probe uid; neither = `SetupDispatch::Skip` and a log line |
| `RecordSighting` | `CatalogOp::UpsertRegisteredDevice(..)` with no association (sight-only) |
| `PushProject` | `DeployOp::PushProject { key, target }` |
| `OpenDeviceHome` | `StudioController::attach_lens` on the target's session |
| `MarkIncompleteFlash` | the card-owned op flow's Failed phase (`CardOp::failed`) |

## 9 · What renders each state (P06)

The wizard is a **card** (flow-spec F5b) — and the G2 gate (2026-08-05)
settled *which* card: **the wizard is a STATE of the device card, not a
card of its own.** One physical board renders as one card at every moment
of the flow. Two frames, one seam:

| Frame | When | What it is |
|---|---|---|
| **Standalone** | no verdict yet: CONNECT_INTRO, BOARD_FIRST, PORT_PICKING, CONNECTING, PROBING — and the whole simulator path until the sim session starts | a card of its own in the entry-cards slot, at the roster's card width, where the setup form used to sit. There is no device card to be the body of. |
| **Takeover** | from the VERDICT on: BOARD_PICK, WLED_FOUND, ALREADY_LP, STALE_LP, PROBE_FAILED, FLASHING, FLASH_FAILED, ABANDON_GUARD, PROVISION | the bound board's OWN roster card renders the wizard as its body: same card, same identity key, same grid slot (pinned first). The header stays the device's and grows real facts as they land; the ✕ moves to the steps rail. |

**Why the verdict is the seam.** Between the port grant and the probe's
answer the live session is anonymous, so a board the registry already
knows would render twice — its remembered card plus a connection card
nothing can merge with it. So the pre-verdict window keeps the wizard
standalone AND stands the bound session's roster row down (the only
suppression in the model; the wizard's own PORT_PICKING/PROBING body is
that window's narration). At the verdict the probe's `hardware_uid` rides
the live row as its `pending_uid`, the live card adopts the remembered
identity, the roster's twin filter drops the registry row, and the wizard
rides the one merged card. An anonymous board has no row to merge with and
rides its session's card.

At DEVICE_HOME and CLOSED the wizard draws **nothing at all**: the handoff
is a body swap, so the card is already there wearing its own body. Nothing
appears, nothing disappears, and "becomes the device card" is literally
true.

One component per state, no flow logic in any of them — every control
dispatches a `SetupGesture` and the reducer decides what it means.
`lp-app/lpa-studio-web/src/app/home/setup_wizard.rs`, one static story per
state beside it.

| State | Card body |
|---|---|
| CONNECT_INTRO | headline + two stacked full-width CTAs; the `ConnectHint` line escalates toward the secondary |
| BOARD_FIRST | the shipped board picker (full catalog, no Generic) + board-specific connect guidance (CH340 driver steps or cable/port tips) + "it's plugged in" |
| PORT_PICKING | indeterminate wait — the browser owns the dialog |
| CONNECTING | indeterminate wait ("Connecting to the board…"), with the op overlay's own TERMINAL under it — from the grant on, the link has lines to show |
| PROBING | indeterminate wait, one line about what is being read, the same terminal under it |
| BOARD_PICK | recognition line, chip-filtered picker + Generic, picked-board bio, the forward verb (armed only when something is picked), Back |
| WLED_FOUND | verdict + the wipe warning (migration is future work) + wipe / keep-WLED |
| ALREADY_LP | registry name + chip, "Done writes nothing and you stay here", Done (primary) / Open in the editor → (secondary) / set-up-fresh |
| STALE_LP | registry name + chip, "this firmware is too old for this Studio", update-the-firmware (the only verb, and it is a flash) / back |
| PROBE_FAILED | the failure, the BOOT-button hint, retry / driver help / back |
| FLASHING | the card-owned op flow's OWN activity view, verbatim; attempt number when > 1 |
| FLASH_FAILED | the detail, "retry re-runs from erase", replug guidance from attempt 2, retry / abandon |
| ABANDON_GUARD | the FLASHING body under the card-resident sheet (keep flashing / abandon) |
| PROVISION | project box (compact line + ⓘ) + derived name field (hardware only) + the forward verb; any §7.10 failure above it |
| DEVICE_HOME | nothing is drawn: the takeover ends and the board's own card body returns while the editor lens attaches (the component is kept for the exhaustive match) |
| CLOSED | nothing is drawn: the flow is over and the card — if the flow had one — is already back to itself |

The two entry cards (**connect a device** / **simulate a device**, half
height, one grid cell) are the only way in; the bare "open the port
chooser" action they replaced is gone.

