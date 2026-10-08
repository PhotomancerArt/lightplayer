---
status: fixed
found: 2026-10-08     # report (Yona, a board connected over Wi‑Fi)
fixed: this change
area: lpa-studio-core device_offers / pending_link_offers × lpa-devices Device (ResetBoard)
class: state-conflation
related:
  - docs/adr/2026-10-07-c6-wifi-link.md
  - docs/adr/2026-10-01-agentic-control-offers-in-core.md
---
# Reset was disabled on a board connected over Wi‑Fi

**Symptom** — On a board connected over Wi‑Fi (a `lan:` link), the card's
Reset was drawn disabled with the words "Reset needs USB". The board
restarts itself over any link when asked. `ClientRequest::Reboot` is
"bridge-independent" and edit-tier on the board, and the relay desk check
restarted a C6 this way 12 times over a network link. So the only thing
stopping it was the card.

**Root cause** — The offer disabled Reset whenever
`DeviceView::is_over_bluetooth()` held. That predicate reads
`firmware_blocked == FIRMWARE_NEEDS_USB`, which the view sets for every
network endpoint (`EndpointKey::is_network`: Bluetooth, the LAN and the
relay). One fact, "this link cannot carry firmware" (no ROM downloader
behind it), was being read as a second one, "this link cannot restart the
board". The second fact is true only of the line reset, and the line reset
was the only route the model had: `Action::ResetBoard` always sent
`LinkCommand::RunReset`, which the WebSocket and GATT links refuse by name.
A Wi‑Fi board also counted as "over Bluetooth", so the misnamed predicate
hid the gap.

**Fix** — The model owns the route. `Device::resets_by_request()` is true
on a network endpoint, and `ResetBoard` there sends the wire's `Reboot`
(`ClientFrame::reboot`) instead of the line pulse. Nothing waits for a
boot: the board answers, resets and drops the link, the transport's own
reconnect brings it back, and the new link identifies like any other. A
board that refuses (below the edit tier, or an embedder that cannot reset)
leaves nothing waiting. In core the offer reads `ResetReach`, not
`firmware_blocked`. `Lines` (USB, serial, a sim) keeps today's line reset.
`Request { author }` is enabled for the author tier and disabled below it
with "Unlock to reset". A pending network link that has not answered says
"Reset waits for the board to answer". `RESET_NEEDS_USB` is gone, and
`is_over_bluetooth`'s docs now say what it really reads.

The card says Reset's reason on its own line UNDER the verb row, not
under the button. The row is one `nowrap` line, so a reason in the
button's column widens that column, and the first, longer wording pushed
Disconnect and Forget past the card's edge (`ble-card-locked`, all three
widths). "Reset needs USB" had fit only by being short.

**Regression coverage** — `lpa-devices` `tests/scenarios.rs`:
`reset_over_a_network_link_asks_the_board_to_restart_itself` (lan, ble,
relay) and `reset_over_usb_still_pulses_the_lines`. `lpa-studio-core`
`studio_device_e2e_tests/lan_reset_tests.rs` presses
`devices/<board>/reset-board` by path in three tests:
`reset_over_wifi_asks_the_board_and_the_card_comes_back`,
`reset_over_wifi_without_the_author_tier_says_why` and
`reset_over_usb_still_pulses_the_lines_and_never_asks`. The offer unit
tests are
`over_bluetooth_the_firmware_verbs_need_usb_but_reset_asks_the_board` and
`over_a_network_link_reset_waits_for_the_board_to_answer`. On the
emulator, `just walk-wifi-emu studio-lan-reset`
(`lp-emu:esp32c6:t1+net=lan`, lp-emu `c79cc6969`): Studio connects by
address and Reset is pressed once. The board's console says
`tick_and_send: reboot acked, resetting`, the ROM prints `rst:0x3
(LP_SW_HPSYS)`, and the board rejoins. A new secure LAN session opens from
the page's own redial, and the card is Ready again with no click.

**Lesson** — A predicate named for a transport ("over Bluetooth") that is
really computed from a capability reason ("firmware blocked") will be read
as whichever of the two the next caller needs. Decide each capability
(flash, reset, update) from the link kind, once, in the model, and project
it. Reading a capability back out of another capability's reason string is
how this happened.
