---
status: fixed
found: 2026-10-07     # the network transport's P05 dev check (behind ?relay=1)
fixed: this change    # network transport PR C, the relay on for everyone
area: lpa-studio-web device_roster_card × lpa-studio-core lan_link_view / access_line
class: state-conflation
related:
  - docs/defects/2026-10-08-reset-was-disabled-on-a-board-on-wifi.md
  - docs/adr/2026-10-08-studio-network-links.md
---
# A board through lightplayer.app said "over Bluetooth" and "USB connected"

**Symptom** — A board reached through the relay (`relay:<mac>`) wore other
links' words on its card. Before a frame landed, the preview said "No live
picture over Bluetooth — Open in editor to see and control it." The
Connections group's USB row said "connected". The login line said
"Connecting over Bluetooth…". Nothing on the card said "Wi‑Fi via
lightplayer.app", which the editor header already said for the same board.
The P05 dev check saw this behind `?relay=1` and left it for the card's
link words (ND6). Turning the relay on for everyone would have shown it to
every user.

**Root cause** — It is the same conflation as Reset's (the related defect).
The card picked its link kind from two facts: "is there a LAN line" (core's
`lan_link_view`, which knew only `lan:` endpoints) and `is_over_bluetooth()`,
which reads `firmware_blocked` and is true on every network link. A relay
board had no LAN line and was firmware-blocked, so the card took it for
Bluetooth. With no LAN line, it also drew the Connections group a USB or
Bluetooth board wears, whose USB row reads "connected" whenever the link is
not Bluetooth. Core's login line was written for Bluetooth alone.

**Fix** — Core's network line (`UiLanLink`) now covers the relay too and
carries the link's kind (`UiLinkKind::Relay`, "Wi‑Fi via lightplayer.app").
The card reads its kind from that line first (`card_link_kind`), so the
preview, the info line and the Connections group follow the real link. A
relay card is treated like a LAN card and wears no USB or Bluetooth rows.
`access_line` takes the link kind, so it says "Connecting over Wi‑Fi via
lightplayer.app…". The update row's words ("Wi‑Fi", "Update nearby once")
are untouched.

**Regression coverage** — `lan_link_view`
`a_board_through_the_relay_says_so_and_nothing_else`;
`device_roster_card` `a_relay_card_says_wifi_via_lightplayer_app_and_never_bluetooth`;
`ui_access_view` `the_login_line_names_the_link`;
`relay_connect_tests`
`a_board_met_over_usb_is_reached_through_lightplayer_app_after_unplug_as_the_same_device`
(the card's line kind is `Relay`); the story `relay-card-connected`; and
`just walk-wifi-emu studio-relay` T3 (`lp-emu:esp32c6:t1+net=lan` through a
local relay, lp-emu `c79cc6969`): the card reads "Wi‑Fi via lightplayer.app
· Open — no password", and the walk fails if it says "over Bluetooth" or
"USB connected".

**Lesson** — This is the second defect in a day from reading a link's kind
back out of `firmware_blocked`. The card should be handed the link kind
(`UiLinkKind`) for every device, not derive it. Here it gets it only for
network links, through the network line.
