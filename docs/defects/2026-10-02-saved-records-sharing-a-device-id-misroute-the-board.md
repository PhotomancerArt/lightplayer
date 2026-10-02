---
status: fixed
found: 2026-10-02      # hardware-walk: Yona's M1 feel gate, recorded with ?record=
fixed: this change
area: lpa-devices roster (load_records) + lpa-studio-core device_roster (load_records, is_already_known) + studio_controller auto-name
class: assumed-context
related:
  - 2026-09-07-merge-delete-erased-the-merged-row.md
  - docs/adr/2026-08-25-event-fold-device-model.md
  - docs/adr/2026-08-04-device-identity-anchored-in-silicon.md
  - lp2025/2026-10-01-1832-wifi-control (M1 feel gate)
---
# Saved records sharing a device id misroute the board, and its auto-name ping-pongs

**Symptom** — Yona's own Chrome profile already remembered the XIAO C6
(`10:bd:a3:b0:8e:30`). On the M1 feel gate the card never reached Ready and
offered no Open. The session recording
(`lp-cli record timeline`, recording `021d922568385f78`) shows the board's
hello identifying it and merging it into its saved record:

```
+0.419s  device:2  IdentityPromoted { binding: Mac, value: "10:bd:a3:b0:8e:30" }
+0.419s  roster    DevicesMerged { from: DeviceId(2), into: DeviceId(1) }
```

but every later frame on that link, starting with the next hello, was
journaled as a contradiction of the same MAC:

```
+0.899s  device:1  IdentityConflict { binding: Mac, value: "10:bd:a3:b0:8e:30" }
```

and `device:1` was renamed `XIAO ESP32-C6 · Oct 2` → `… · Oct 2 2` → `… · Oct 2`
every 50–80 ms, each rename followed by `LibraryChanged`, 243 times until the
recording ended at +22 s. The renames kept going after the port was lost.
Fresh headless profiles never reproduced it, because they have no saved rows.

**Root cause** — two registry rows carried the same model handle,
`device_id: 1`: the C6's (MAC-keyed and unnamed) and another board's. Both
loaded as roster entries wearing `DeviceId(1)`, and the model reaches a device
**by id** (`Roster::index_of`, the link `routes` map, `Action::SetName`), so
every input addressed to `DeviceId(1)` went to whichever entry came first:

- The pending link's hello matched the C6's record **by MAC**, so the merge
  went to the right entry. The link was then routed **by id**, so every later
  frame folded into the other board, whose chain holds a different MAC. The
  result was an `IdentityConflict` on every frame, and a C6 entry that never
  heard its board again.
- The auto-name (`settle_device_records` → `auto_name_actions`) saw the C6
  record still unnamed and sent `SetName { device: 1 }`, which renamed the
  OTHER entry. On the next settle the C6 was still unnamed, its derived name
  was now taken by the neighbour's title, so it got `… 2`, which freed the
  base name again. The `taken` list also counted the card being named, so
  even a correctly routed rename was not guaranteed to be a fixed point.

Why the duplicate id existed: a row's `device_id` is minted per page from 1,
and `DeviceRoster::is_already_known` treated "some device wears this row's id"
as "this row is loaded", without comparing identities. So if a link mints
number N before the library hydrate lands, the saved row with `device_id: N`
is skipped. A different board promoted under N is then persisted with N too,
and from then on the registry holds two rows with one id. Two tabs minting
independently gets you to the same place. The model assumed persisted ids
were unique and never checked.

The identity rules themselves are correct and unchanged: a pending MAC that
matches a saved record merges, and here it did. The conflict was a misroute,
not a rule. **Link speed does not matter:** the duplicate came off disk, and
the records hydrate before the sweep attaches links. The faster
`claude/c6-link-io-thread` board only made the second hello arrive sooner.

**Fix** —

- `lpa-devices` `Roster::load_records` now guarantees one id per entry. A
  record whose id is already held by a device, a pending link, or an earlier
  record of the same batch is loaded under a freshly minted id. The mint is
  first raised past every id in the batch. It returns the ids it loaded
  under, and the next persist writes the new id back to the row.
- `lpa-studio-core` `DeviceRoster::load_records` keys each row by the id it
  was actually loaded under. `is_already_known` matches a row by its registry
  key (MAC-keyed rows included; the old check compared uids only, which
  missed every board Studio flashes), and by handle only when the device
  wearing it does not contradict the row's uid or MAC.
- `taken_device_titles` leaves out the card being named, so deriving a name
  for a card that already wears it returns the same name.

**Regression coverage** —

- `lpa-devices` `roster::tests::records_sharing_an_id_load_apart_and_the_board_keeps_its_own_frames`
  (normal and fast hello; on main it journals the recording's two
  `IdentityConflict`s).
- `lpa-devices` `roster::tests::a_record_loaded_after_a_link_minted_its_id_takes_a_fresh_one`
  (the origin; on main the ids come out as `[1, 1, 2]`).
- `lpa-studio-core` `studio_device_e2e_tests::a_remembered_board_comes_back_to_its_record_and_is_named_once`
  (the session end to end over the fake board's real bytes; on main it times
  out waiting for Ready, the neighbour "Porch sign" is renamed
  `XIAO ESP32-C6 · Jan 1`, and the C6 card wears its bare MAC). Beside it,
  `a_single_remembered_board_comes_back_to_its_record` passes on both, which
  pins the precondition.
- `device_roster::tests::rows_sharing_a_handle_load_as_two_boards_and_stay_loaded_once`,
  `device_flash::tests::naming_a_card_again_is_a_fixed_point`.

**Lesson** — An app-side handle that is persisted becomes a claim about
identity the moment anything reads it back. Here it was read back as one
(`row.device_id == device.id` ⇒ "same board") and as a routing key, while
nothing ever minted it with that guarantee. A handle that crosses a reload
needs one of two things: whoever loads it enforces uniqueness, or it is never
compared as identity. This fix does both. Separately, any loop that writes a
derived value and then re-derives it on the next settle has to be a fixed
point on its own output. A uniquifier that counts the thing being named is
one misroute away from oscillating forever.
