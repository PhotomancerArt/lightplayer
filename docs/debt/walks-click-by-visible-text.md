---
status: carried
since: 2026-09-10      # best effort: the walks have clicked by words since the emulator-first walk
logged: 2026-10-09
area: scripts/emu walks, scripts/device-scenario.mjs, spikes/ble-lab
related:
  - ../../scripts/emu/studio-driver.mjs
  - ../../scripts/emu/emulated-lane.mjs
  - ../../scripts/emu/walk-wifi-emu-lan.mjs
  - ../adr/2026-10-08-the-board-card-and-one-home-page.md
  - ../adr/2026-09-10-the-emulator-first-device-walk.md
---
# The walks find Studio's controls by their visible words

**Shape** — the walks drive Studio's real page in headless Chrome, and most
of them name a control by the text on it (`click("Flash firmware")`,
`clickWhenReady("to choose from")`) and a card by the words inside it
(`cardOf`'s "Wi‑Fi · <address>" climb). `StudioDriver.click` matches a
*substring* of any button or link on the whole page. So a rename breaks a
walk, and so does a new section that repeats a word: the walk finds a
different control, or none, and says so at minute eight of a ten-minute
lane. No CI job runs a walk (`pre-merge.yml` runs `tab-smoke.mjs`, which
never loads Studio), so nothing tells the author of a page change which walks
they just broke.

**Carrying cost** — the one home page changed nine walk scripts, the two files
they share, `device-scenario` and a spike's README for a handful of labels, a
route and an address row that moved. Each lane then needs a bundle build and
a firmware package before it can show whether the edit was right, so the
check is a long local run, or it is skipped and the walk is found red later.
`clickWhenReady` also waited on *any* control with the words while `click`
then looked inside a scope for another, so a wait could be satisfied by the
wrong control (fixed 2026-10-09).

**Workarounds**
- Name a control through a hook the page owns, not through its words: an
  element id (`#home-connect-board`, `#home-online-boards`,
  `#home-offline-boards`), each a `pub(crate) const` whose comment names the
  walks that read it.
- `StudioDriver.pressConnect("USB" | "Bluetooth" | "Network")` presses a
  Connect a board square by its whole word inside its section;
  `openNetworkRow` opens the address row once (the square toggles it).
- `click`/`clickWhenReady` take `{ scope, exact, nth }`, and the wait asks
  the same question the click will: use `exact` and a `scope` whenever the
  word is short or common.
- Before a long lane, check the page's words in seconds: serve the bundle
  with `serveStudioBundle`, open `/`, and look at `driver.controls()` for a
  word the walks click with no scope.
- When a page change touches labels or ids, `git grep -n` the old word in
  `scripts/` and `spikes/ble-lab/` before the change leaves a draft.

**Incident log**
- **2026-10-09** — the one home page (plan
  `lp2025/2026-10-08-2050-one-home-page`, P08): "via USB" / "via Bluetooth"
  became squares, the address field moved behind the Network square, the
  remembered line's `show` toggle and its sentence went, `/devices` became
  `/`, and the Devices nav link became the logo. Fixed with the hooks above.
  The lanes were not run in the implementing worktree (it had no release
  bundle, firmware package or `lp-cli` build); the PR says who ran them.
- **2026-10-09** — the board card (plan `lp2025/2026-10-08-2050-the-board-card`,
  P09): every board-card read moved onto the card's own hooks
  (`data-board-card`, `data-bar`, `data-bar-work`, `data-board-corner`,
  `data-board-terminal`, `data-offer-path`) through `StudioDriver`'s card
  helpers: a verb is pressed by its offer path, ready is core offering
  `push` or `edit`, and the board's words come off its terminal. Running
  the lanes before and after found three lanes already red on `main` for
  reasons that are not the card: `walk-wifi-emu usb`/`ble` stop at "open"
  (they assume a board whose Wi‑Fi cannot connect; today's firmware can),
  every `walk-migration-emu` scenario but W2 stops building its fixture
  (today's image reaches the pre-2026-10 filesystem at 0x310000), and
  `walk-drop-emu`'s editor never opens over an `emu serve` door
  (`docs/defects/2026-10-09-the-editor-never-opens-on-an-emu-serve-usb-board.md`).

**Exit criteria** — every walk addresses Studio through a hook the page
owns (an id or a `data-` attribute, as the board card's `[data-board-card]`
is meant to be), a shared contract probe checks those hooks in CI against
the built bundle, and a page change that moves one fails that probe rather
than a lane.
