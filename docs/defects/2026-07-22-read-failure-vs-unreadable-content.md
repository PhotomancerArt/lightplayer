---
status: open
found: 2026-07-22      # how: live-debugging
area: lpa-studio-core/roster
class: state-conflation
related: ["2026-07-17-unreadable-masqueraded-as-empty", "2026-07-22-identity-lost-on-failed-read", "Planning/lp2025/2026-07-20-runtime-pool/"]
---
# A failed read renders as "Holds unreadable data"

**Symptom** — When a content read *fails* — e.g. the old-firmware hash
bug (`2026-07-22-littlefs-listdir-doubled`) — the card renders "Holds
unreadable data". That claims the *device* holds bad content, when
actually *Studio* failed to read it. The device's content may be
perfectly fine.

**Root cause** — `DeviceContent::Unreadable` models both facts:
"content exists but doesn't parse" and "the read operation failed".
They are different facts with different affordances — unparseable
content invites *replace/erase*; a failed read invites
*retry/update-firmware*, and offering erase on a transient read failure
is offering data loss as the remedy for a client-side bug.

**Fix** — None yet. Queued for the device domain-model review
(`Planning/lp2025/2026-07-20-runtime-pool/` context); the review will
decide whether this becomes a new `DeviceContent` variant plus a
distinct card state, or is resolved some other way.

**Regression coverage** — None; the distinction doesn't exist in the
model yet, so there is nothing to pin.

**Lesson** — Inherited conflation resurfaces.
`2026-07-17-unreadable-masqueraded-as-empty` split "empty" from
"unreadable" but left "unreadable" itself conflating device-fact with
client-fact — and five days later the partial-knowledge fix
(`2026-07-22-identity-lost-on-failed-read`) had to route failed reads
*into* that conflated state to have anywhere to go. Registered open so
the model review has it on the docket rather than rediscovering it.
