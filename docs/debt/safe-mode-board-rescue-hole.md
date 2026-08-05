---
status: carried
since: 2026-08-04      # observed on the Zook dome bench, project-format-upgrades vision Q3
logged: 2026-08-04
area: lp-cli/lpa-link upload (device transport) + project-format migration
related:
  [
    "../adr/2026-08-04-project-format-migration-architecture.md (D14: device upgrade = pull → migrate-in-library → push)",
    "../adr/2026-07-04-crash-recovery-model.md (safe-mode boot: transport/server come up, engine does not fully run)",
    "library-format-migration-gap.md",
  ]
---
# A board wedged in safe mode with an old-format project cannot be rescued by the Upgrade flow

**Shape** — The Upgrade verb this repo now has
(`../adr/2026-08-04-project-format-migration-architecture.md`, decision 7)
depends on being able to pull a board's project, migrate it in the
library, and push the result back. `lp-cli upload` cannot reach a board
that is in safe mode: heartbeats flow (transport and server come up per
the crash-recovery model), but an upload session reports `responses=0`
and hangs rather than completing — measured on the Zook dome bench
(esp32v3, `DOM-Z-102`), not yet root-caused. If a board is BOTH wedged in
safe mode AND holding a project at a format this build cannot load, the
two failure modes compound: the amber "Holds old-format project" card's
one non-destructive affordance (Upgrade: pull → migrate → push) cannot
complete its push leg, and the only other affordance (Wipe) is
destructive.

This was flagged, not solved, at the vision stage of the migration plan
(`vision.md` Q3: "If bad state + old format coincide, the pull→migrate→push
loop has a gap") and deliberately left out of that plan's scope, to be
registered as debt with a trigger rather than guessed at without a
reproducing board.

**Carrying cost** — Nothing today, because no board has hit both
conditions at once. If one does, the honest card this plan built
(`RosterCardState::HoldsOldFormatProject`) still shows the right verb, but
dispatching it will hang or fail against a board the transport cannot
actually push to — which will read as a bug in the *new* Upgrade flow
rather than the *pre-existing* safe-mode upload gap it actually is.

**Workarounds** — From the one bench occurrence of the (unrelated) upload
side of this hang (`zook-dome-silicon-verdict` memory, 2026-08-04): erase
`lpfs` (`0x310000 0xF0000`) and `espflash reset` to force the board back
to an idle (non-safe-mode) boot, then upload normally. That workaround is
**destructive to the board's stored project** — it is a firmware-recovery
move, not a project-preserving one, so it does not actually solve the
"rescue an old-format project trapped behind safe mode" case this entry
is about; it only gets the board bootable again.

**Incident log**
- 2026-08-04 — flagged at the project-format-upgrades vision stage (Q3);
  no reproducing board seen yet. The upload-hangs-on-safe-mode symptom
  itself was observed earlier the same day on the Zook dome bench, in an
  unrelated context (flashing a build, not rescuing an old format).

**Exit criteria** — Either: (a) `lp-cli upload`/the push transport is
fixed to reach a safe-mode board (root-cause the `responses=0` hang), so
the ordinary Upgrade verb's push leg works unconditionally; or (b) a
dedicated recovery path is built that can write a migrated project to a
safe-mode board without a normal upload session (e.g. through the
bootloader-mode recovery path,
`../adr/2026-07-30-bootloader-mode-detection.md`, which already reaches
boards a live session cannot). Trigger to prioritize either: the first
real field occurrence of a board that is both in safe mode and holding a
project below `PROJECT_FORMAT_VERSION`.
