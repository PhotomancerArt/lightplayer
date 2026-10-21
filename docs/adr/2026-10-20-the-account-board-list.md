# ADR: The account's board list

- **Status:** Accepted (plan approval, Yona, 2026-10-19: "all yes"; ship gate pending)
- **Date:** 2026-10-20
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Related:** [2026-10-06-cloud-relay](2026-10-06-cloud-relay.md) (the relay,
  its picture cache, `ListBoards`; amended 2026-10-20 to point here),
  [2026-10-08-studio-network-links](2026-10-08-studio-network-links.md).
  Planning: `lp2025/2026-10-19-2305-account-board-list` (`plan.md`,
  `notes.md`), M2 of the roadmap `lp2025/2026-10-19-2300-your-boards-follow-you`.

## Context

After M1, a board showed on another computer only while it was online at
lightplayer.app's relay: presence lives in the relay's memory, so an offline
board was absent, and its last picture was lost at every deploy. A board set
up on a phone and unplugged did not exist on the computer. The roadmap's
promise is that a board that ever joined the account shows on every computer
the account signs in on, offline included, with its name, when it was last
seen, its firmware, its project and its last picture.

The cloud's store (`MetaStore`, SQLite in production) held users, account
access, sessions and projects, and no boards. The relay hub is sans-IO and
never touched the store; the one store call on the relay's path was the
account proof at registration.

## Decision

**The account keeps one record per board** (`BoardRecord`, the
`account_boards` table, migration `0007_account_boards`), keyed by
`(account, board MAC)`:

| Column | What |
|---|---|
| `label` | the board's own name, as it reports it (roadmap D5): never a browser's rename |
| `model` | the board's catalog model, from a Studio report only (D7) |
| `firmware`, `project` | the last reported (the project by name) |
| `joined_at` | when it joined this list |
| `seen_relay_at`, `seen_usb_at`, `seen_ble_at`, `seen_lan_at` | when each kind of link last saw it (D9); "last seen" is the newest |
| `picture_at`, `picture_outputs`, `picture_colors`, `picture_accounts` | the last picture and its readers, all set or all NULL (D1) |

The table is additive; nothing is backfilled (lists fill as boards next
reach the relay or a Studio). It is not a project format:
`PROJECT_FORMAT_VERSION` and the browser's `/registry.json` are unchanged
(account rows stay in Studio's memory).

**Joining.** A board joins the list of every account it proves at the relay
when it registers (the relay's proof, current key salts only), and the list
of the signed-in account when Studio reads the board's access list — over
USB, or over Bluetooth or the LAN once unlocked at edit — and finds the
account's **current** key there (`ReportBoard`, once per link per
connection, D5). The fact that joins is "the board holds your key", not "a
Studio just installed it": Bluetooth installs nothing by itself, and a board
keyed elsewhere still belongs on the list. The relay's own link never
reports; the relay writes its own sightings.

**Merging facts (D11).** Label, firmware and project take the newer fact;
the model only from a report that names one; each sighting keeps its newer
time. A source says only what it knows: the relay's protocol 1 hello carries
no firmware and keeps the last one; a Studio that has not heard the
board's project says nothing about it; the relay's own `Project` frame is
the one source that can say "no project loaded".

**Leaving (D4, D8).** Only Forget removes a record: **Remove from your
account** (`devices/<board>/forget-from-account`, `ForgetBoard`). It removes
no key from the board — that stays the Access row's job — and a board that
reaches the relay again with the account's key rejoins. A key reset, or a
key removed in the Access row, leaves the board listed. Forget is not
revoking.

**The cap (D3).** At most 64 records per account (`MAX_ACCOUNT_BOARDS`,
one constant in `lpc-cloud-api` that the online cap and the answer size
share). At the cap a new board does not join: the relay still lets it
online (the cap is the list's, not the relay's) and logs it, and
`ReportBoard` answers the named refusal `BoardListFull { max }`, which the
board's card says once: "Your account holds 64 boards. Remove one to add
this one." A board already on the list always updates.

**A record grants nothing (D6).** Presence, pictures and routes stay gated
by the accounts the board proved to the relay. A report adds a row to the
caller's own list and nothing else. A record is read only by its own
account. A stored picture carries the accounts the board had proved when
the picture was taken, and only they read it — the record holding it is not
enough.

**The picture, written behind (D1).** The relay writes a board's last
picture to its accounts' records when the board leaves, when the relay goes
away (graceful shutdown, before legs close `GoingAway`), and at most every
ten minutes while it is online — checked when a picture arrives, never a
timer of its own, never per picture. A crash loses at most ten minutes of
picture and the leave time; the next registration fixes the facts.
`BoardPictures` answers a board the process holds no picture of from the
reader's own record; a stored picture's `seq` is its `at` in milliseconds,
a range no live `seq` (numbered from the process start in microseconds)
ever reaches.

**The hub stays sans-IO (D12).** It decides *when* a record changes and
returns `BoardRecordWrite`s (`Seen`, `Project`, `Picture`; `take_writes`);
the edge applies them through the store (`apply_board_record_write`), on the
board leg's own task, after the hub's lock is released.

**No relay-protocol change (D7).** The relay hello stays as it is:
`RELAY_PROTO_VERSION`, the relay goldens and `lpc-relay` are untouched. The
board's model is not in the hello; a record the relay alone wrote has none
until a Studio meets the board. Putting the model in the hello is left for
the next relay protocol that has other reasons to exist.

**Cloud API v7 (D2).** `ListAccountBoards` answers every record, merged with
the relay's presence for the boards the caller proved (`online`), online
boards first, then by last seen; an online board the caller proved with no
record yet is listed from its presence, so online boards never disappear.
`ReportBoard` and `ForgetBoard` answer `Ack` (`ForgetBoard` also for a board
that was not listed). `ListBoards` and `BoardList` are deleted, and every
caller moved, in the same change. `CLOUD_API_VERSION` is 7.

**Studio (D10, D13).** An account board offline everywhere is an offline
card in Offline boards on every computer: "This board is offline. It was
last seen 2 hours ago." over its kept picture, dimmed; its rows the last
known facts, dimmed; lightplayer.app `<email> · 2 h ago`; no
Connect. A board this browser remembers takes the record's facts only where
the record saw it more recently. On an account board the card has two
forgets: **Forget on this browser** (the existing `forget`, its copy saying
the board still shows from the account) and **Remove from your account**
(Lasting, two clicks, the agent never presses it).

## Consequences

- An account's boards follow it to every computer, offline included, and a
  board's last picture survives a deploy.
- The cloud holds new persisted data about boards (`account_boards`): names,
  models, firmware versions, project names, sightings and a small picture
  (≈800 B). It is the account's own and read only by it.
- A reconnect storm after a deploy writes twice per board (the proof, and the
  join), and the shutdown writes once per online board.
- The 64-board online cap is four times the old 16.
- Studio reports a board on every connection that reads its list; the call
  is idempotent (a merge) and cheap.

## Alternatives Considered

- **Extend `ListBoards` instead of replacing it.** Every API bump refuses
  old tabs anyway, and Studio deploys with the server; two calls that list
  boards would drift. Replaced (D2).
- **Join on install.** Bluetooth installs nothing by itself, and a board
  keyed elsewhere would never join; "the board holds your current key" is
  the fact (D5).
- **Remove the board's key on Forget.** Keys are the Access row's; the list
  forgetting a board must not lock anyone out (D4).
- **A key reset drops the board from the list.** It would leave a board the
  account owns invisible exactly when the account needs to re-key it (D8).
- **Write the picture on every picture, or never.** Every picture is up to
  four writes a second per watched board; never loses the picture at every
  deploy. Written behind (D1).
- **The model in the relay hello.** A relay-protocol change, and a
  never-break pin, for one field (D7).
- **The hub calls the store.** It would make the relay's hot path wait on a
  file and the hub untestable without one (D12).

## Follow-ups

- The account owns a board's name (roadmap future work).
- The model in the relay hello, with the next relay protocol.
- The best link and the automatic move onto the LAN (roadmap M3); the
  extended emulated walks (M4). Both done, 2026-10-21: M4's lane `just
  walk-wifi-emu studio-relay-account` walks a board joining the list over
  USB with no Wi‑Fi, two boards offline on a never-met profile with their
  last facts, and Remove from your account.
