---
status: fixed
found: 2026-07-22      # how: hardware-walk
fixed: c1841349e
area: lpa-studio-core/home + device
class: partial-knowledge-loss
related:
  ["2026-07-22-identity-lost-on-failed-read", "2026-07-22-read-failure-vs-unreadable-content"]
---
# Reconnect briefly spawned a twin card during the connect window

**Symptom** — Clicking Reconnect on a remembered device card
("TestBoard1") made a second, anonymous card pop into existence for
the duration of the connect, which then collapsed back into the
original card once the identity read landed. (Second sighting of the
class after `2026-07-22-identity-lost-on-failed-read` fixed the
steady-state case: the remaining twin lived only in the connect
WINDOW, before any identity exists.)

**Root cause** — `DeviceOp::ReconnectDevice` carried no device uid,
even though the user clicked a SPECIFIC remembered card. During the
connect window the live evidence (flow Connecting, no sync) rendered
as its own card with `uid: None`, so the roster could not attribute it
to the remembered card until the identity read finished. Knowledge the
gesture itself carried was discarded at dispatch.

**Fix** — `ReconnectDevice { uid }`; `DeviceController` holds
`pending_reconnect_uid` while the flow is open (cleared on flow
reset/recover/disconnect); the roster builder attributes an
identity-less live card to the pending remembered card (uid, name,
transport adopted), so the connect narration renders in place. The
identity read remains the truth once it lands.

**Regression coverage** —
`reconnect_window_renders_on_the_remembered_card_not_a_twin`
(home_view_builder).

**Lesson** — Gestures carry knowledge: an action initiated FROM an
entity should keep naming that entity through its async window.
Same class as discarding facts in error paths — here the fact was
discarded at dispatch time instead.
