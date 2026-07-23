---
status: fixed
found: 2026-07-22      # how: live-debugging
fixed: 89474a789 (on branch claude/runtime-pool-m4; merges with the device-flow PR)
area: lpa-studio-core/places+studio
class: partial-knowledge-loss
related: ["2026-07-22-read-failure-vs-unreadable-content"]
---
# Failed content read discarded the identity already learned

**Symptom** — Reconnecting to the remembered TestBoard1 spawned a
*second*, anonymous card — "Connected device", amber — instead of
reusing the existing named card, with the note:

```
could not read the device: protocol error: hash package failed…
```

One physical device, two roster cards, and the one with the live link
had lost its name.

**Root cause** — `pull_device_copy` read the device identity
*successfully*, then the content hash failed (the old-firmware
`hash_package` bug, see `2026-07-22-littlefs-listdir-doubled`) and the
wholesale error path threw away everything the flow had already
learned: `identity: None` → no uid → the roster couldn't dedup the
live connection against the registry card it demonstrably matched.

**Fix** — Content-read failures *after* the identity read return a
`PulledDeviceCopy` carrying the identity plus a `read_error`, instead
of an error that erases the pull. `absorb` preserves the identity,
records the sighting, and classifies the content Unreadable; the card
dedups against the registry entry and keeps its name.

**Regression coverage** —
`failed_read_live_card_keeps_its_identity_and_dedups_the_registry`
(home_view_builder).

**Lesson** — Error paths must model partial knowledge. "The operation
failed" and "we know nothing" are different statements, and a
`Result<Everything, Error>` return type can't say the first without
saying the second — the success payload has to be able to carry a
partial result with the error inside it. Any multi-step read flow that
returns all-or-nothing will eventually erase step 1's facts because
step 3 failed.
