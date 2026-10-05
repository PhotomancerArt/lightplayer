---
status: fixed
found: 2026-10-02      # how: report (Yona's desk XIAO ESP32-C6, 10:bd:a3:b0:8e:30)
fixed: this change
area: lpa-studio-core app/access (`sync_access`, `AccessController::apply` Synced arm); lpc-access `DeviceAccessFile::upsert_secret`
class: capped-store-without-eviction
related:
  - docs/adr/2026-10-02-two-passwords-open-by-default.md
  - docs/adr/2026-09-24-easy-bluetooth-access.md
  - spikes/access-panel-tidy/index.html
---
# A full device store refuses new access silently

**Symptom** — Yona's desk C6 listed sixteen entries, every one
`kind: browser`, `tier: edit`, labelled "Brave on Mac". Plugging it in from
a new worktree's Studio added nothing and said nothing: no toast, no error
in the access panel, and that origin's browser could not unlock the board
over Bluetooth afterwards.

**Root cause** — two things, each harmless alone. (1) A browser's key lives
in `localStorage` (`lp.access.browser.v1`), which is per origin, and every
agent worktree serves Studio on its own hashed port (`scripts/dev-port.sh`),
so every worktree is a new browser that mints a new key, and each one's
first USB connect installs it. (2) The board holds at most
`lpc_access::MAX_SECRETS_PER_FILE` (16) entries and refuses the 17th
(`TooManySecrets`), and Studio's sync answered that refusal with only a
`log::warn!` in `AccessController::apply`'s `Synced` arm. So the store
filled once and then quietly stopped taking anyone new. The sync also added
before it removed retired account keys, so a full board could not even
take a rotated account key in place of its old one.

**Fix** — automatic room. Before an add that needs a slot on a full device,
both the USB sync and a panel change drop the `browser` entry added longest
ago (no time counts as oldest) — never a key this browser holds, never an
account's entry, never a password (`device_access_ops::make_room`). The
toast or the panel names what was dropped. Retired keys are removed before
anything is added. A sync that still fails (nothing can be dropped) puts
its sentence in the access panel instead of only the log. The panel folds
the sixteen identical rows into "Brave on Mac ×11".

**Regression coverage** —
`access_controller::tests::the_seventeenth_origin_drops_the_oldest_browser_and_says_so`
(seventeen origins, one FakeBoard running the real access store),
`device_access_ops::tests::a_full_device_drops_its_oldest_other_browser_to_make_room`,
`…::a_full_device_of_passwords_and_account_keys_says_so`,
`…::retired_keys_go_before_the_new_ones_come`.

**Lesson** — a capped store fed by an identity that multiplies on its own
(one per origin, per install, per worktree) fills on a schedule nobody
chose, so the cap needs an eviction rule from day one, and a refusal at the
cap must reach a person. Yona's board file was not edited; the fold and the
automatic room make it usable as it stands.
