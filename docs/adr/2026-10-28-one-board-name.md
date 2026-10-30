# ADR: One board name, everywhere

- **Status:** Accepted (Yona, 2026-10-28: "If I rename it, I expect that name
  to stick everywhere, including on the bluetooth"; plan questions "all yes")
- **Date:** 2026-10-28
- **Deciders:** Photomancer
- **Supersedes:** the rule that a board's name is Studio's own and is never
  written to the board: [2026-08-04-device-identity-anchored-in-silicon](2026-08-04-device-identity-anchored-in-silicon.md)
  ("Name ... registry-only data"), [2026-10-20-the-account-board-list](2026-10-20-the-account-board-list.md)
  (`label` "never a browser's rename") and `docs/style/language.md`.
- **Superseded by:** None
- **Related:** [2026-09-23-ble-access-model](2026-09-23-ble-access-model.md)
  (the advertised name), [2026-10-08-the-board-card-and-one-home-page](2026-10-08-the-board-card-and-one-home-page.md)
  (the name bar). Planning: `lp2025/2026-10-28-2051-one-board-name`.

## Context

Yona flashed a board, put the PLAYFUL Choker Tryout example on it and
renamed the board "Flower-Viatrix". The project kept the example's name, he
could not find where to rename it, and Bluetooth still advertised
`LP-studio` (the folder every Studio push lands in). Three names for one
thing, and only one of them was his.

Until now a board's name lived only in the browser that gave it
(`/registry.json`). The board advertised its project folder, and the
account's board list stored whatever name the board reported, which was
none.

## Decision

1. **A person's name for a board is the board's own name.** Studio writes
   it to the board, and the board keeps it in the existing `name` field of
   `/.lp/device.json`. That file and field already existed and were already
   read (the relay hello names the board from it), so no persisted shape
   changes: a write keeps a stamped `uid` and holds `{ "uid"?, "name"? }`.
   A name-only file is a named, unstamped board.
2. **The wire:** `ClientRequest::SetDeviceName { name }` (edit tier) and its
   answer `ServerMsgBody::SetDeviceName { name }`; the hello carries
   `deviceName`. `WIRE_PROTO_VERSION` 47. Every end cleans a name the same
   way (`lpc_wire::clean_device_name`: controls dropped, whitespace
   collapsed, 64 characters at most).
3. **When Studio writes it:** a rename (the name bar, the push picker's
   "name the board the same", a name typed at install) is kept as *unsent*
   (`Intent::name_unsent`, persisted additively in the registry row) until
   the board answers. It is sent at once when the board is connected at
   this Studio's wire and no activity holds the link; otherwise at the
   board's next hello. A board on an older wire is never sent it; it gets
   the name the first time it connects after an update.
4. **Which name wins:** an unsent rename is written over whatever the board
   says. With nothing unsent, the name a board reports in its hello is the
   title, so a rename made in another browser (or with `lp-cli`) shows
   everywhere.
5. **Studio's minted names stay Studio's.** The automatic name a board gets
   from its model ("XIAO ESP32-C6", `Action::SetName`) is never written to
   the board. Only a name a person gave (`Action::NameBoard`) is.
6. **Bluetooth says the name.** The board advertises `LP-<its name>`, else
   `LP-<project folder>`, else `LP-<last four MAC hex>`, cut on a character
   to the 26-byte advertising budget. A rename is advertised within the
   5-second refresh, with no reboot. The GAP device name is fixed when the
   GATT server is built, so it takes the name at the next boot.
7. **The project follows the board.** What lands on a board from the
   examples or the library is a copy named after the board (a library
   project stays as it is; pushing a board its own project again is an
   update, not a copy). Renaming a board renames the project it plays when
   it is the only board playing it, and renaming such a project renames its
   board. A project several boards share keeps its name when one of them
   is renamed, and nothing says so: many boards on one project is not a
   case Studio supports yet.
8. **The project is renamed where its name is read:** the editor's project
   panel title is a button with a pencil, like the board's name bar.

## Consequences

- A board shows the same name to every browser, to a phone scanning for it
  and to lightplayer.app's board list (whose `label` is the board's own
  name, now the one a person gave it).
- Boards in the field are on wire 45 until updated; their names stay
  per-browser until then, and are written on the first connect after.
- The relay hello reads the name at boot, so lightplayer.app sees a rename
  after the board's next restart.
- A name typed before this change is not marked unsent, so it stays in its
  browser until the board is renamed again.

## Alternatives considered

- **A new file for the name** (`/.lp/name.json`). Rejected: a new persisted
  format, when the identity file already had a `name` field every reader
  understood.
- **Studio writes `/.lp/device.json` with a plain filesystem write.**
  Rejected: a read-modify-write over the wire races the board, and the
  board's own Bluetooth name could not follow without a reboot.
- **Board wins on every connect.** Rejected: a rename made while the board
  was away would be undone the moment it came back.
- **Say "plays on N boards; its name stays" on a rename.** Rejected by
  Yona: robotic, and about a case Studio does not support.
