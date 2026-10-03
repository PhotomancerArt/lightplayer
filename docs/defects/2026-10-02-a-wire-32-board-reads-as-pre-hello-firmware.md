---
status: fixed
found: 2026-10-02      # how: hardware-walk (G1 of the C6 repartition, the spare XIAO C6)
fixed: this change
area: lpa-link device_link/demux × lpc-wire hello (wire 33) × lpa-devices evidence
class: partial-knowledge-loss
related:
  - docs/adr/2026-07-14-wire-hello-versioning.md
  - lp2025/2026-10-01-1843-c6-repartition (G1-F1)
---
# A wire-32 board reads as "pre-hello firmware" in a wire-33 Studio

**Symptom** — G1, the branch's Studio (wire 33) and the spare XIAO C6 on
#891's firmware (wire 32), heartbeats arriving and its project running:
the card said "No LightPlayer hello — pre-hello firmware", "no firmware",
"Pre-hello firmware — needs firmware", with the board picker, instead of an
older LightPlayer to update. Every fielded board is on wire 32, so every
user's first sight of this release would have been that card. `lp-cli link
capture` on the same board printed the cause beside the hello:
`a message did not parse: missing field fs at line 1 column 499`.

**Root cause** — wire 33 made the hello's `hardware.fs` required. A
wire-32 hello therefore fails the full `ServerMessage` decode, and the
demux turned the failure into an anomaly (`LinkEvent::Error`) — dropping
the one fact the version handshake exists to deliver: the `proto` was
right there in the bytes. With no hello in the window and heartbeats
flowing, the fold reached the verdict for firmware that never says hello.
The hello-versioning ADR's rule ("absence of a hello is the mismatch
signal") assumes a host can always see a hello's version; a breaking change
to the hello itself is exactly when it could not. The emulator walk could
not show it: its old-layout boards run the branch's own wire-33 firmware.

**Fix** — `lpc_wire::hello_proto` reads ONE field, `msg.hello.proto`, from a
message the full decode rejected — no other field of an older hello, so no
second decoder for an old shape. The demux turns a hello from another wire
into `ServerFrameBody::HelloOnOtherWire { proto }` (a hello claiming this
wire that does not decode stays an anomaly; an app conversation's reply
stays the conversation's). The fold keeps it as a hello: from an older wire
the verdict is `OlderLightPlayer { proto: Some(32) }` — "Older LightPlayer
firmware — flash to update; the project stays" — at once, without waiting
out the identify deadline; from a newer wire, a LightPlayer this Studio is
behind (`BoardNewer`). And an older LightPlayer whose board resolves now
offers the running-board verb, one-click **Update firmware**, falling back
to Flash's pick when nothing names the board.

**Regression coverage** — the hello captured off the spare board at wire
32 is `lp-core/lpc-wire/testdata/hello-proto32-xiao-c6.json`:
`lpc-wire` `hello_proto` tests (it does not decode at wire 33, and names
32); `lpa-link` `demux::tests::a_hello_from_another_wire_reaches_the_fold_as_its_version`
(+ this-wire and app-range cases); `lpa-devices`
`evidence::tests::a_hello_from_another_wire_is_a_light_player_on_that_wire`;
and end to end from the bytes to the card,
`lpa-studio-core` `device_flash::tests::a_wire_32_board_is_older_light_player_firmware_that_updates`.

**Lesson** — the version field is the one part of the hello that must be
readable by every other version, and a typed decode of the whole message
does not guarantee that: the first required field added to the hello made
the version invisible to the very peers it was for. Read the version
first (or on failure), by itself, and keep everything else typed.
