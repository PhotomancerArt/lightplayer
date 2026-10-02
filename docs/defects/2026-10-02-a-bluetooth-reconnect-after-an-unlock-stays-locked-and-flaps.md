---
status: fixed
found: 2026-10-02      # how: report (Yona, Bluefy on iPhone against the XIAO C6 choker, #891 firmware)
fixed: this change
area: lpa-studio-core app/access (`AccessSession::logged_in`)
class: state-conflation
related:
  - docs/adr/2026-09-24-ble-transport.md (S3 silent reconnect, S6 login on connect)
  - docs/defects/2026-09-23-bluefy-hidden-page-does-not-see-ble-drops.md
  - docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md
---
# A Bluetooth reconnect after an automatic unlock stays locked, so the link flaps forever

**Symptom** — Studio at lightplayer.app in Bluefy on an iPhone, connected over
Bluetooth to `LP-PLAYFUL` (XIAO ESP32-C6 `10:bd:a3:b0:8e:30`, firmware from
#891) while the Mac's Studio held the same board over USB. Controls worked,
but Bluefy kept putting up its own native alert, "LP-PLAYFUL disconnected".
The link came back and then dropped again, over and over. Each alert is
modal, so the phone was unusable.

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

**Not settled here** — what caused the *first* drop. This fix stops a drop
from turning into a loop, but each real drop still gets one Bluefy alert,
which is native and outside the page's reach. The board logs every disconnect
with its HCI reason (`[ble] …: disconnected, reason=0x..`) and every close it
asks for (`closing at the server's request (…)`). Comparing main with #891
firmware, with and without a USB session, needs those lines and a Bluetooth
central. That means `spikes/ble-lab` on Mac Chrome once Chrome has macOS
Bluetooth permission, or a phone sitting.

**Lesson** — a guard that rations attempts should count failures, not
attempts. "Spent" tied to the act of trying also catches success, and on a
transport that reconnects silently, a success that is not repeated becomes a
failure on the next link. Under a silent-reconnect design, ask of every piece
of per-device state whether it still holds on the next link.
