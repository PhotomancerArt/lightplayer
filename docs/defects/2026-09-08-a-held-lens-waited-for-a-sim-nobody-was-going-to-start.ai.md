---
status: fixed
found: 2026-09-08      # production, main at 9e1e71e63 (after the P5 merge) — reported by Yona: no project could be opened
area: lp-app/lpa-studio-core/src/app/studio/studio_controller.rs (try_pending_device_lens, resolve_open_device, seed_device_sim_records)
class: open-path-wait-without-wakeup
related: [lp-app/lpa-studio-core/src/app/devices/sim_transport.rs, lp-app/lpa-studio-web/src/web_app.rs, lp2025/2026-09-07-0118-studio-emulated-boards, 5aa8dae55, 872959a5f, 4c64cdfce]
---
# The editor waited forever for a sim that nothing was ever going to power on

**Symptom** — On production, clicking any project landed on the editor and
stayed at "Opening…" indefinitely. The studio console said, once, and then
nothing more:

```
[studio] waiting for the device before opening it: missing session: this board is not connected
[studio] Waiting for the device
```

A `fw_browser` worker in the same tab *did* boot and *did* load a project
(`Loading project: projects/preview` → `Project loaded: preview` → the
shader compiled), which made it look as though the runtime was alive and
only the UI was stuck. It was not the same runtime: `preview` is the
gallery's own preview host (`preview_host_impl.rs`,
`PREVIEW_PROJECT_ID = "preview"`), which boots for thumbnails whatever the
open does. The sim the editor was waiting on had never started at all.

**Root cause** — Two holes on the same path, both of which end in a wait
with no wake-up.

1. **The resolution read remembered sims out of a map that had not been
   filled yet.** `resolve_open_device` picks the sim to open on out of
   `self.device_sims`, which is seeded from the library at the first
   settle (`seed_device_sim_records`). A cold page whose first gesture is
   the open reads "no sim of this target" off a map that is merely
   empty-so-far and MINTS one — so every cold open left another scratch
   record behind. The freshly minted record then has to be adopted by the
   roster and powered on within the same open, and any step of that which
   does not land leaves the open holding.

2. **`pending_device_lens` had no way to start the thing it was waiting
   for.** The hold is right for silicon — a board is on a desk and may be
   plugged in a minute from now — but a sim only ever runs because this
   tab started it. `try_pending_device_lens` re-checked
   `device_lens_attachment` on every tick and kept holding while it
   failed, and `sim_that_did_not_start` (the release valve added in P3)
   returns `None` for a sim that is **not powered**, on the reasoning that
   an unpowered sim is one the open has not got to yet. So the one state
   nothing could leave was: held lens, sim off, nobody starting it.

   The open path's own power-on could be skipped silently, which is how
   that state was reached. P4 narrowed it to sims with
   `let is_sim = self.device_sims.contains_key(&uid)` — correct in intent
   (`Connect`ing silicon closes a wire that was working) but it made the
   power-on depend on the sidecar map a second time, and
   `sim_session_for` — which `power_on_sim_for` needs to build a session —
   depends on it a third. `seed_device_sim_records` REPLACED that map from
   each snapshot, dropping any uid whose `/device-sims/<uid>.json` did not
   read back, so a sidecar the snapshot could not produce unmade the sim
   for the whole open path. The `/device/<uid>` address is the same hole
   with no power-on upstream of it at all: `RuntimeOp::OpenDeviceLens` on
   a sim that is off held forever by construction.

**Fix** — The hold wakes its own sim. `try_pending_device_lens` now calls
`wake_held_sim` first: a held device that is a sim and is off is powered
on through the transport (arming the sweep, exactly as the card's Power on
does), the hold stands while it boots, and a sim that **cannot** be
started at all ends the hold with a verdict instead of waiting for it.
Silicon is untouched — waiting is the right answer for a board.

Three narrower repairs stop the state being reached in the first place:
`resolve_open_device` settles the library before concluding there is no
sim to reuse; `seed_device_sim_records` keeps the record this tab already
holds when a snapshot cannot read the sidecar back (only the row going
away — Forget — removes it); and the sim/no-sim decision plus
`sim_session_for` now accept the registry row's own `transport: "sim"`,
`board_id` and `hardware_id` as the witness when the sidecar is missing —
the same two facts, written a second time by `new_sim_record`.

**Regression coverage** — in `studio_device_e2e_tests.rs`:
`a_cold_reload_opens_on_the_remembered_sim_instead_of_minting_a_second`
(hole 1: verified red — the reload minted a second record and then hung),
`a_held_lens_on_a_sim_that_is_off_powers_it_on_and_lands` (hole 2, the
address path: verified red — "the held lens never woke its sim"), and
`powering_the_sim_off_and_reopening_the_project_starts_it_again` (the
Power-off-then-reopen walk; green before and after, so it is a guard
rather than a repro).

**Lesson** — A wait is only honest when something can end it. The device
model's holds were written for boards, where the world supplies the event
the wait is for; a sim has no world outside the tab, so a hold on one is a
hold on an event this program is itself responsible for producing. Every
`pending_*` that waits for a resource the app owns needs to answer "who
starts it, and what happens if they already didn't?" — and the answer must
not be a state that reads identically to "any second now".
