# ADR: Bluetooth access, easy by default — generated keys, plugging in is access

- **Status:** Proposed
- **Date:** 2026-09-24
- **Deciders:** Photomancer
- **Supersedes:** in part, items 6 and 7 of
  `2026-09-24-ble-transport-studio.md` (log in with an account default
  password; the device store written whole, never read)
- **Superseded by:** None

## Context

The BLE slice (`2026-09-23-ble-access-model.md`, `2026-09-24-ble-transport.md`,
`2026-09-24-ble-transport-studio.md`) shipped access with typed passwords.
Every step needed a person: turn Bluetooth on over USB, choose a password,
type it on the phone. Studio kept the passwords it had written and rewrote
the whole device store from them, so a second browser erased the first
browser's password, and nobody could see what was on a device.

Yona set the rule for this work (2026-09-24, verbatim):

> the guiding principle here is simplicity. most of the time, users
> shouldn't have to think about access control. we should make it really to
> see what has access. an account name. a device. whatever. physical
> connection = access. bluetooth/remote access should be one-click/easy as
> possible. they shouldn't have to enter any passwords or think about it.
> only if they really want to put a manual password in should they need to.

And earlier the same day: "This is _not_ high stakes stuff. Bluetooth
should be really easy to enable, too."

The threat model is unchanged: someone cheeky within radio range, not a
cracker with the flash chip on a bench.

The design is the converged spike `spikes/ble-easy-access/index.html`
(branch `claude/wizardly-robinson-e45721`, gate passed 2026-09-24); the plan
is `lp2025/2026-09-24-1953-ble-easy-access`.

## Decision

### Key holders: a browser, an account, and account passwords

A device's list holds keys of three kinds (`SecretEntry.kind`):

- **This browser.** Each browser mints one 32-byte random secret and one
  16-byte salt, once, and keeps them in `lp.access.browser.v1` with a name
  ("Yona's MacBook" when signed in, else "Chrome on Mac"; renameable).
- **Your account.** The signed-in account has its own random secret and
  salt, minted by the cloud (`GetAccountAccess`, `ResetAccountKey`).
  Guests get none.
- **Account passwords.** Optional, one play and one edit, none by default
  (`SetAccountPassword`). Each has its own fixed salt in the account
  record. They replace the old "Default device password" setting.

Shared passwords (typed on a device's list, or sent as a Share link) stay
as they were: a label, a tier and a per-password salt.

Each holder uses **one salt on every device**, and generated secrets use
`iterations: 1` (a 256-bit random secret needs no stretching; a human
password keeps Studio's PBKDF2 cost). The login protocol does not change.
A client knows its own offers in a challenge by salt, so the automatic
unlock answers once with the key that matches and never guesses: it cannot
trip the board's backoff. When nothing matches, no answer is sent, and the
sheet ("Unlock `<device>`" / "This device needs a password to unlock it")
answers the challenge the board left open. While the sheet is up, each
new connection begins one challenge and holds it for the password being
typed, so the board sees a login in progress rather than an idle link
(the board keeps such a link only as long as its firmware allows; see
Run M in the BLE plan's walk record).

### Physical connection is access

Over USB (a trusted link) Studio adds what is missing without asking:
this browser's key and, when signed in, the account key and any account
passwords. A toast names what was added and offers Undo. It does this on
someone else's device too, because plugging in is the permission. A
renamed browser key is re-labelled on the next connect (same salt
replaces), and a reset account key's old salt is removed.

### The board keeps the list, and lists it

The board merges changes itself (`AccessList`, `AccessAdd`,
`AccessRemove`, `AccessSetSwitches`, all edit tier; see the 2026-09-24
amendment of `2026-09-23-ble-access-model.md`). "Who has access" is read
from the board, so it shows every key on the device, whoever added it, and
two browsers never erase each other's keys. A link must hold **edit** to
see the list. The list never carries `k` or `iterations`.

### Bluetooth on by default

A device with no device store starts Bluetooth, locked, with no keys. A
damaged store keeps Bluetooth off. Over USB the card's Bluetooth switch
applies by restarting the device, and Studio does the restart; over
Bluetooth the switch is locked ("turn off by USB"), since you cannot turn
off the radio you are talking over.

### Account keys and passwords live in the cloud, and are cached

The account record (`AccountAccess`, cloud API 4) holds the account key
and the two optional passwords. The passwords are stored readable so
Settings can show them (D7): they are shareable device passwords, like a
Wi-Fi password, not account credentials. After sign-in Studio caches the
account's keys in `lp.access.account.v1`, so a phone with no internet (at
camp) still unlocks.

### Project sidecars stay on the board, and Studio stops writing them

The board still honours `<project>/.lp/access.json` (v1 and v2): a key in
a loaded project's sidecar still unlocks. Studio no longer shows or writes
them. The project Bluetooth section is gone, and so is its plumbing in
Studio core. Keys live on the device, so the device's list is the one
place to look.

### Words

Unlock, device, "Device password", "Who has access". No screen says "log
in" about a device, so no one types their account password into it.

## Alternatives considered

From the spike's first round (commit `537f0b51d`):

- **The old password form** (three decisions before anything works), **one
  big "Turn on Bluetooth" button**, or **a set-up checkbox**. All folded
  into: Bluetooth is simply on by default.
- **The old "passwords this browser wrote" list with "Replace all"**, a
  one-sentence summary, or chips for the access list. Rejected for a list
  read from the board, with every key on it.
- **Asking before adding keys to someone else's device.** It breaks
  "physical connection = access".
- **Share as words without the QR**, or typed only. Words plus a QR won;
  typing your own is still there as "Type my own instead".
- **A single default device password.** Replaced by optional play and edit
  account passwords.

From planning:

- **A per-device salt for every holder.** Every browser would need a record
  for every device it has touched, and an automatic unlock on a device it
  has no record for would have to guess. See Revisit below.
- **Typed passwords only.** That is the shipped slice, and it is what
  Yona's rule replaces.

## Consequences

- An unlock the user never sees: a phone holding a matching key (its own,
  the account's, an account password, or a remembered one) unlocks with no
  screen, and the card says "Unlocked by `<name>`".
- One salt per holder is visible before login, so an observer can tell two
  devices share a holder, and one device's flash dump opens that holder's
  other devices. Accepted for now; see Revisit.
- The access files went to `version: 2` (entry `kind`, `addedAt`), with v1
  still read; `WIRE_PROTO_VERSION` went to 25 and `CLOUD_API_VERSION` to 4.
  `PROJECT_FORMAT_VERSION` is untouched.
- Every emulated C6 with an empty filesystem now boots with its BLE
  controller up (about 24 KB of heap; the heap ledger was re-baselined).
- Unlock over `?ble=emu` cannot be walked, because the emulated link is
  trusted. The unlock oracle is Studio-core's host tests against `FakeBoard`,
  which runs the board's real access store and verifier.

## Revisit: one key per holder, on every device

Yona, 2026-09-25: "probably fine, but we should note it somewhere as a
future enhancement, any security analysis would flag that for sure. its an
acceptable decision now, but we probably should revisit it."

A browser or an account installs the same `(salt, K)` on every device it
touches. So anyone in range can link two devices to one holder by the salt
in their challenges, and anyone who dumps one device's flash holds a key
that opens every device of that holder. Any security review will flag
this.

The likely fix is a per-device key derived from the holder's one secret,
`K_dev = HMAC(secret, device uid)`, with a fresh salt per device, matched
by the device's uid from the hello. One dump then opens one device, and no
per-device record is needed in the browser. It is a new entry shape and a
wire change, so it needs its own plan. The model ADR's amendment carries
the same note: `2026-09-23-ble-access-model.md`, "Generated keys, one salt
per holder" → "Revisit".
