# ADR: Board ids, and offers that take typed parameters

- **Status:** Accepted (2026-10-02, by Yona; the decisions and their words
  are below)
- **Date:** 2026-10-02
- **Deciders:** Yona
- **Refines:** `docs/adr/2026-10-01-offer-tree-and-consequence-levels.md`
- **Evidence:** planning dir
  `lp2025/2026-10-01-1255-agentic-ui-roadmap/m3-devices-as-offers/`
  (`plan.md`, `notes.md` with the answers and both gates, P1–P5
  Implementation Results); PR #925.

## Context

The offer-tree ADR put the project header and node cards in one tree,
addressed by path, and left two things for the devices page.

**A device verb needs a stable name for the device.** An offer path is the
id the web, the agent and the ⌘K palette share. The model's own handle,
`DeviceId(u64)`, looked like the obvious segment (`devices/3/flash`), and
the Wi-Fi roadmap's PR #896 had already built paths that way. But a
`DeviceId` is minted from this browser profile's saved records, so the same
board has a different number in another browser. It can also be reused:
the counter is seeded from the highest saved id, so after the newest board
is forgotten and the page reloads, the next board gets its number. A path
the agent remembers, or one a person pastes, could then name another board.

**Device verbs take values.** Flash needs *which board*. Push needs *which
project*. Rename needs *the name*. The lists those values come from were
already core's (`flash_offer`, `push_offer`, `target_offer`), but the value
the user picked lived in web signals, and the web built the op from it. An
offer could not say "I need a board", so the agent could not fill one in,
and a card handing a Lasting flash to the user could only show a plain
button.

## Decision

### One board id, MAC-shaped, with its kind in the path

`BoardKey` (`lp-app/lpa-devices/src/board_key.rs`) is a board's id
everywhere Studio names one: its six MAC octets, written as 12 lowercase
hex digits (`6055f90a0b0c`). It parses every spelling the code already
holds (colons, dashes, upper case). It refuses the all-zero and all-ones
addresses, because that is what a failed efuse read looks like.

- **Real boards use their silicon MAC**, from the hello or the flash
  preflight's efuse read.
- **Sims and emulated boards get a generated one.**
  `BoardKey::locally_administered` sets the IEEE locally administered bit
  and clears the multicast bit. Vendor hardware never ships with that bit
  set, so a generated id can never be a real board's. It is minted once,
  from caller-supplied random bytes (sans-IO), and kept in the sim record's
  `base_mac`.
- **`lp-cli emu serve` boards no longer wear the desk C6's MAC.** Board `n`
  defaults to `02:4c:50:00:<n>`, so an emulated board and the real desk
  board are never one entry in a browser that has seen both. `mac=` on a
  `--board` still overrides it.

The path segment is a `BoardRef` (`app/devices/board_ref.rs`), which names
its kind as well as its id:

| Segment | What it is |
|---|---|
| `mac-<12 hex>` | a board known by its silicon MAC; also an `emu serve` board, which Studio reaches through the serial shim exactly as it reaches a real one |
| `sim-<12 hex>` | an in-browser sim (a `sim:` endpoint), by its generated MAC |
| `emu-<12 hex>` | an in-tab emulated board (an `emu:` endpoint), by its generated MAC |
| `new-<n>` | a link or board that has not said who it is yet, by its roster handle; it becomes one of the others once it does |

The kind comes from the endpoint scheme, which is studio-core's, so it is
decided there and not in `lpa-devices`. Device offers live at
`devices/<board ref>/<verb>` (`devices/mac-6055f90a0b0c/push`). The add
slot's verbs sit directly under `devices/`: `connect-usb`, `connect-ble`
and `new-sim`.

**No persisted format changed.** Records already held the MAC text the
hello reports, and every sim sidecar since v1 carries `base_mac`. A key is
read off that text wherever an id is needed, so no stored byte changes
shape and old records load unchanged.

### Offers declare typed parameters

`UiOffer` carries `params: Vec<OfferParam>` (`core/offer/offer_param.rs`).
Each parameter has a `name` (the key a press carries), a `label`, and one
of three kinds:

- **`Choice`**: one of a closed list core computed (`OfferChoice`: value,
  label, detail, a `disabled` reason, never hidden), with an optional
  `preselect` when there is an obvious pick.
- **`Text`**: a placeholder, an optional `max_len`, and whether it may be
  left out.
- **`Toggle`**: on or off, defaulting to its current state.

**The binder.** A parameterised offer holds an `OfferBinder`, a closure
core builds that captures what it already resolved (the device, the boards
a chip fits). It turns the user's values into the op. A renderer and the
agent hand over only the values. Neither of them builds an op.

**Validation is one function.** `UiOffer::press(&OfferArgs)` refuses a name
the offer does not take, a value that is not an enabled option, text over
its limit, and a toggle that is not `true`/`false`. It fills in defaults,
refuses a required value still missing, and then binds. Every refusal is an
`OfferArgError` that names the parameter and the choices, in words the
agent reads back and acts on. The action that comes out is stamped with the
offer path and the args (`OfferPress`), so a press from a card and a press
from the device card are recognised as the same offer.

**`only_with`.** An option the list is narrowed away from by default (a
board outside the detected chip) names the toggle that widens the list
(`all_boards`). A renderer draws it only while that toggle is on, and a
press that picks it with the toggle off is refused. The filter is a
convenience. The flash preflight's chip guard is still the safety.

Verbs that use parameters: Flash and Update firmware (`board`, optional
`name`, `all_boards`), Push (`source`, an optional `name` for a new
project, `name_board`), Rename (`name`), Autoconnect (`enabled`), and New
sim (`board`, `backing`).

### One offer with a choice list, not one offer per choice

Flash is **one** offer whose `board` parameter lists the boards. It is not
one `flash-<board>` offer per board. Yona, 2026-10-02: "one is better -- I
agree". Every parameterised verb works the same way. The tree stays one
entry per verb a person sees. The readout says "takes board: one of …"
once, instead of repeating the verb eight times. A choice list that changes
(a chip is detected, a project is saved) changes one offer's options rather
than adding and removing paths the agent may have remembered.

### Renderers draw parameters; the web never builds the op

The web draws a parameterised offer with `OfferParamsForm`
(`lpa-studio-web/src/core/offer/offer_params_form.rs`): a picker for a
choice, a field for text, a switch for a toggle. It presses through
`UiOffer::press`. The device pickers (`BoardPickPanel`, the push picker)
are renderers of the same parameters. The web-actions ratchet records the
device files at 0.

### The chat card renders the real control, pre-filled

When the agent presses a Lasting offer, or one that needs a real click, it
becomes a card. The card carries the offer path and the agent's args, and
draws that offer's own control from the live tree, set to the agent's
values: the board pick for Flash, `OfferParamsForm` for anything else. The
user may change a value before pressing. The press is the offer's own
binding of what they settled on. Core recognises any press of that offer as
the card's answer and tells the agent what the user changed. Yona,
2026-10-02, at the P4 visual gate: "that looks good to me".

`act` takes `{action, args, why}`. A bad value is refused with every choice
listed. A press missing a required value is refused rather than turned into
an empty card.

### Bluetooth reach is a core fact

Whether "via Bluetooth" can work in this browser is reported by the web
into core (`StudioCommand::BluetoothReach`), as `usb_available` already
was. `BluetoothReach` (`app/devices/bluetooth_reach.rs`) holds the decision
and its sentence. `devices/connect-ble` is always published, and is
disabled with that sentence when Bluetooth is not usable (Off, Brave's
flag, iOS, Firefox, Safari, or not asked yet). The add slot and the agent
read the same reason. The way forward the slot adds under it (a link to
Bluefy, the flag to copy) stays the web's.

### Levels that depend on the board

- **Flash a blank board: Routine** (Q3). A blank chip loses nothing, so it
  is one click and the agent may press it. Flash over anything else
  (somebody's firmware, an older LightPlayer, a chip that will not say) is
  Lasting.
- **Push** (Q4): Routine onto an empty board, and over a project the
  library holds. **Lasting** over a project the library has no copy of,
  because that project would be gone. Push is not offered until the board
  has said what it runs, so a push is never aimed at a guess.
- **Reset board stays published while busy, but disabled**
  (`RESET_WAITS_FOR_ACTIVITY`). The device model refuses a reset under an
  activity, and Cancel is the way out. Over Bluetooth it is disabled
  because there are no reset lines.

Q2–Q7 and D8 were answered "rest yes" by Yona on 2026-10-02. The board-id
scheme went through three answers that day:

1. Q1: "I think mac makes sense, and we may well want to generate fake macs
   for emu and sim so we can have one common id type."
2. Design gate: "I liked calling out that the id is mac-based … maybe
   sim-1234 and emu-1234 to differentiate them?"
3. The emulator default MAC: "yes".

## Consequences

- An offer path names the same board in every browser and survives forget
  and reload. A board's path changes once, from `new-<n>` to its MAC ref,
  when it first says who it is.
- A new device verb that takes a value declares `params` and a binder in
  core, and gets the web's form, the agent's readout ("takes board: one of
  …"), validation and the card for free.
- `DeviceFeedOp` (the card's live-picture lease) is plumbing, not an offer.
  It is allowlisted in the web-actions ratchet, and the ratchet now also
  counts `SomeOp{…}.into_action()` (Q5, Q6).
- The agent's readout now says what each board runs (`running "<label>"`,
  `no project loaded`), so it can confirm a push landed (E4).
- PR #896 (the Wi-Fi roadmap's layout-update verbs) built decimal
  `devices/<DeviceId>/…` paths. Whichever of the two merges second adopts
  `BoardRef`.

## Alternatives Considered

- **`DeviceId` in the path** (`devices/3/flash`, the Q1 lean before Yona's
  question). Rejected: it differs per browser profile and can be reused
  after a forget and a reload.
- **A bare MAC with no kind** (`devices/6055f90a0b0c/…`). Briefly adopted,
  then replaced at the design gate. Every made board has a locally
  administered MAC, and so can an `emu serve` board, so the hex alone does
  not tell a sim from silicon. The prefix does, and it reads better.
- **One offer per choice** (`devices/<ref>/flash-<board>`). Rejected by
  Yona (above).
- **The web keeps the picked value and builds the op**, as before.
  Rejected: an op the web builds is invisible to the agent and untestable
  from core, which is the parent ADR's whole point.

## Follow-ups

- The app chat window is M5. Until then the card renders in the existing
  chat transcript renderer and in stories.
- A Lasting press with a required value missing is refused. It could
  instead become a card that asks the user for the value.
