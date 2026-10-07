---
status: fixed
found: 2026-10-06      # how: hardware walk (the M7 agent pre-walk, Studio in Mac Chrome over Bluetooth, fixture C6)
fixed: this change
area: lpa-studio-core `StudioController::drive_device_updates` (the update host's credentials) × the access controller's remembered passwords
class: untested-path
related:
  - docs/adr/2026-10-06-studio-updates-over-the-update-channel.md
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (P5, P12, P13)
---
# A board unlocked with a typed password cannot log in to its core-only half

**Symptom** — Studio, fresh browser, a locked board over Bluetooth: the
Unlock sheet took the typed password, the card offered "Install …", and
the press started the update. The board reset into core-only (its first
leg: the engine's header erased), asked the core-side login (`N`/`A`), and
Studio's update ended `LoginRefused` within a second. The board sat
core-only with its transfer pending; the card kept reading "Finishing the
update… 0%" (filed separately:
`2026-10-06-the-card-holds-finishing-after-a-refused-login.md`).

**Root cause** — the update host's credentials were the keys this browser
and account hold (`AccessController::held`). A Bluetooth unlock with a
typed password installs no key, so the board knows none of them; the
password itself was remembered by the access controller but never handed
to the update host. The host tests covered a held key and an unknown key,
never a password.

**Fix** — the host's credentials are the held keys, then the remembered
passwords (`lpa_update::Credential::Password`). New e2e test
`a_core_install_over_an_untrusted_link_logs_in_with_a_remembered_password`.
Re-run on the fixture: the same flow logged in to core-only and the core
went on.

**Still open** — a browser that unlocked with a password it did NOT
remember, or that never unlocked (a board found core-only), holds nothing:
the plan's future work names a password prompt inside the update flow.
