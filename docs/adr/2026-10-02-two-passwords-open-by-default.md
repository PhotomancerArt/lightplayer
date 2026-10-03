# ADR: Device access is two passwords, and a new board is open to anyone nearby

- **Status:** Proposed
- **Date:** 2026-10-02
- **Deciders:** Photomancer (Yona)
- **Supersedes:** in part, `2026-09-24-easy-bluetooth-access.md`
  ("Bluetooth on by default": a new board was locked; the "Who has access"
  list with named shared passwords and the Share sheet) and the `open`
  rule of `2026-09-23-ble-access-model.md` (D17: open grants play, never
  edit)
- **Superseded by:** None

## Context

Easy Bluetooth access shipped on 2026-09-25: plugging in by USB installs
this browser's key, a board with no store is locked, a person can add
named passwords from a Share sheet, and "Anyone nearby can play" is a
switch. A week of real use found three problems (Yona, 2026-10-02):

- **The list got long.** A browser's key is per origin, and every agent
  worktree serves Studio on its own port, so Yona's desk C6 filled its 16
  slots with "Brave on Mac" and then silently refused anyone new
  (`docs/defects/2026-10-02-a-full-device-store-refuses-new-access-silently.md`).
- **Custom passwords were hard to enter**: a name, a tier, a sheet.
- **"Anyone nearby" did not do what was wanted.** It could only open play.

The guiding principle, in Yona's words: "I just really want this to be
simple … this isn't enterprise banking software." And: "a human coder
would probably have been like … default password 1234 you can change it
if you like end of story. that's what wled does."

The design is the converged spike `spikes/access-panel-tidy/index.html`
(concept 4B, section 1; PR #929), five rounds with Yona.

## Decision

### Two lines: Play and Author, each Anyone or Password

The access panel starts "Who nearby can…" with two lines. **Play** (the
panel, brightness, patterns) and **Author** (everything but firmware — the
edit tier; "Edit sounds like you're editing the password") are each
**Anyone** or **Password**. Anyone who can author can play, so while Author
is Anyone, Play follows it.

Password shows a box with a random `word-word-NN` password already in it.
Clicking in selects all of it, so typing replaces it; ↻ rolls another; it
saves when you click away (or on Enter). Anyone saves at once, with no
confirm. Replacing a password removes the old entry and adds the new one
at a fresh salt. The device's password for a tier is every `password`
entry at that tier the signed-in account does not own; setting it replaces
them all with one entry ("Play password" / "Author password"). A board
keeps only a derived key, so Studio shows a password only if this browser
set it (remembered per device beside the remembered passwords); otherwise
it says so and offers to replace it.

The Share sheet, its QR and named shared passwords are retired. The
`/unlock` page stays, so links already shared still land.

### `open` is a tier, and a new board is open at author — for now

The device store's `open` becomes `OpenTo { nobody, play, edit }`
(`lpc-access`). A link holds the higher of what its login granted and what
the board is open to. **A board with no store is open at edit**
(`DeviceAccessFile::FRESH_OPEN`, one constant). Yona: "for development and
alpha testing, I want public access — like WLED still does … It's not hard
to change the default later." The device card says it where it is seen:
its access row reads "open to anyone nearby", warning-tinted, and a
Bluetooth link that got in that way reads "Open to anyone nearby".

**An existing board keeps exactly what it had.** The store goes to
version 3 with v2 and v1 readers: v2's `open: true` (which only ever
granted play) reads as `play`, `false` as `nobody`. Nothing is silently
widened. A damaged store is still locked with Bluetooth off.

Wire: `AccessList.open` and `AccessSetSwitches.open` carry the word;
`WIRE_PROTO_VERSION` 33.

### Keys are a footnote, folded, and make their own room

Under a separator, one line — "Your browsers & account · always get in ·
N of 16 · added by USB" — opens the keys, folded by kind and name ("Brave
on Mac ×11", with its date span; one two-tap trash can removes the group).
When a board is full and something new needs a slot, Studio drops the
browser key added longest ago — never one this browser holds, never an
account's entry, never a password — and the toast or the panel says which.
Yona on a friend's browser being the one dropped: "I know and I just don't
care."

## Consequences

- A board fresh from flashing is usable from any phone nearby with no
  setup at all, including authoring. That is a deliberate exposure for the
  alpha; the card says so on every open board.
- Studio's sync never fails at the cap again for want of room from other
  browsers; when it cannot make room it says why in the panel.
- A person picks between two fixed passwords, never names one. Older named
  passwords count as the device's password for their tier until replaced.
- `/.lp/access.json` is version 3; boards read v1–v3. Studio's cached
  listing (`lp.access.device-lists.v1`) from an older build reads as empty
  and is re-read on the next connect.

## Alternatives Considered

From the spike (`spikes/access-panel-tidy`, rounds 1–5):

- **Rounds 1–3: the list as the subject**, grouped, with an "Anyone nearby"
  switch and the passwords under it; "Keep newest" / "Make room" buttons.
  Rejected: nobody would know what "keep newest" means — room is automatic.
- **4A, two switches with a summary sentence**, and **4C, three levels**
  (open / play open / both locked) as one choice. 4B's Anyone | Password
  per line read most directly.
- **Locked by default** (the 2026-09-24 decision). Rejected for now: "with
  such a tiny group of people using it, that's a better default."
- **Per-device keys instead of per-origin keys** would also shorten the
  list; not needed for this, and still a revisit item of
  `2026-09-24-easy-bluetooth-access`.

## Follow-ups

- Flip `DeviceAccessFile::FRESH_OPEN` back to `nobody` (or `play`) when
  LightPlayer leaves alpha.
- The access panel is not yet on the offer tree
  (`2026-10-01-agentic-control-offers-in-core.md`); it moves with the
  devices surface (agentic-UI roadmap M3).
- Desk check of open-at-author over BLE on a real board (the emulated BLE
  link is trusted, so it cannot prove access).
