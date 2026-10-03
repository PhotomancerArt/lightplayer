---
status: fixed
found: 2026-10-02      # how: live-debugging (reading the access flow while chasing a Bluefy flapping report)
fixed: 26958de2c
area: lpa-studio-core app/access (`AccessSession::logged_in`)
class: state-conflation
related:
  - docs/adr/2026-09-24-ble-transport.md (S3 silent reconnect, S6 login on connect)
  - docs/defects/2026-09-23-bluefy-hidden-page-does-not-see-ble-drops.md
  - docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md
---
# A Bluetooth reconnect after an automatic unlock stays locked, so the link flaps forever

**Symptom** — found by reading the code while investigating Yona's report
(2026-10-02): Bluefy on an iPhone, Studio over Bluetooth to `LP-PLAYFUL`
(XIAO ESP32-C6 `10:bd:a3:b0:8e:30`, #891 firmware), with the Mac's Studio
holding the same board over USB. Bluefy kept putting up its own native
"LP-PLAYFUL disconnected" alert, the link came back, and it dropped again.
**This defect is not what flapped that board.** Its store is `open: true`, so
an untrusted link holds play from the start (`AccessState::tier` falls back to
`device_open`), the board never closes it for a missing unlock, and Studio
never enters the loop below. On a **locked** board (Bluetooth on, not open,
the default since easy access) the loop runs exactly as described: one
native alert per lap, every 10–30 s, forever.

**Root cause** — `AccessSession::logged_in` set `auto_spent` after **every**
automatic login, including one that worked. A held key (this browser's key,
the account's) is only offered while `!auto_spent`. So the first link was
unlocked silently, but after any drop the silent reconnect (ADR S3) came up
as a new link holding nothing and Studio never offered the key again. Instead
`checked` raised the Unlock sheet (`NoPasswordKnown`) and held a challenge
open for it. The board dropped the locked link at its unlock deadline (10 s,
held up to the challenge's 30 s life, `link_mux_transport.rs`). Web Bluetooth
reconnected in 250 ms, and that link was locked as well. One drop of any kind
(radio, iOS, a hidden page) therefore became a loop with no end, and Bluefy
put up an alert on every lap. `auto_spent` had two meanings: "an automatic try
was made" and "an automatic try was refused". Only the second should stop the
next try. A held key is matched by salt and never guesses, and a remembered
password that worked will work again.

**Fix** — only an automatic try that did not unlock is spent (`logged_in`
skips `auto_spent` on `Granted`). Every new window is unlocked again with one
answer and no sheet. Refused tries are still spent once per device, so a
silent reconnect never burns the board's backoff
(`refused_automatic_tries_prompt_and_are_not_repeated_on_reconnect` still
holds).

**Regression coverage** — `a_reconnect_after_an_automatic_unlock_is_unlocked_again`
(state machine, three windows) and
`every_silent_reconnect_is_unlocked_again_with_the_browser_key` (controller,
against `FakeBoard`'s real `LoginState`, with the new `FakeBoard::drop_link`
giving each reconnect a link that holds nothing). The controller test fails
without the fix (`link 2: left None, right Some(Edit)`). `?ble=emu` could not
have caught this: the emulated board's Bluetooth link is its trusted USB link,
so it never asks for an unlock (ADR S5).

**What flapped the open board** (settled later the same day) — not this
defect, and not the USB/radio frame-buffer lead this entry first pointed at.
It was Studio's write chunking: Bluefy sends a typed-array view's whole
buffer, so any request over 512 B, such as a palette pin, was refused as a
long write and the page tore the link down
(`docs/defects/2026-10-02-bluefy-writes-a-views-whole-buffer.md`). The
frame-buffer contention is real but separate: on the spare XIAO it showed
`io task silent` and fps drops on main and on #891 alike.

**Lesson** — a guard that rations attempts should count failures, not
attempts. "Spent" tied to the act of trying also catches success, and on a
transport that reconnects silently, a success that is not repeated becomes a
failure on the next link. Under a silent-reconnect design, ask of every piece
of per-device state whether it still holds on the next link.
