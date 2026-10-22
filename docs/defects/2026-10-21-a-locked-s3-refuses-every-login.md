---
status: fixed
found: 2026-10-21      # how: the S3 radio plan's M3 desk walk, locking FS3 over Bluetooth from Studio
fixed: 2026-10-21
area: lp-fw/fw-esp32s3/src/main.rs (the server's entropy source)
class: config-masked-defect
related:
  - docs/adr/2026-09-24-ble-transport.md
  - lp-app/lpa-server/src/access_state.rs
---
# A locked S3 refused every login

**Symptom** — FS3 (XIAO ESP32-S3 Plus, the `ble` image) took a password for
Author from Studio over Bluetooth, and from then on could not be unlocked: on
a new link the card read "play only", the unlock sheet took the password and
sat on "Unlocking…" for as long as it was left open. The board's console said
nothing about the login.

**Cause** — `lpa-server` mints a login challenge's nonce from an entropy
source the embedder injects (`LpServer::set_entropy_source`; the server is
sans-IO and never draws randomness itself). With none it answers every
`Login` with `Error: login is unavailable: this server has no entropy
source` (`access_state.rs`, `begin_login`). The C6 injects its hardware RNG
(`fill_random`); the S3's `main.rs` copied the C6's server setup before
Bluetooth existed on it and never gained the line. Over USB nothing showed it:
the USB link is trusted and never logs in. The emulator walk could not show it
either: `?ble=emu` rides the emulated board's trusted USB link, so every
request there is answered at the edit tier.

**Fix** — the S3 gets the C6's `fill_random` (`esp_hal::rng::Rng`) and passes
it to `set_entropy_source`, unconditionally, as the C6 does.

**Lesson** — a board that grows a radio link grows an access gate, and the
gate has inputs the trusted link never needed. When a chip gains a link,
diff its server setup against the chip that already has that link, setter by
setter. Studio's sheet staying on "Unlocking…" when the board answered with an
error is a separate Studio gap (filed to the auto-queue).
