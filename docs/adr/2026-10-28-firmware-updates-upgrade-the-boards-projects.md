# ADR: A firmware update upgrades the board's projects

- **Status:** Accepted (Yona's format-gate ruling G1, item 9, 2026-10-27,
  reversing the earlier lean that a board refuses old projects and Studio
  offers an Upgrade button)
- **Date:** 2026-10-28
- **Deciders:** Yona (G1), Photomancer
- **Supersedes:** None. It narrows decision 7 of
  [2026-08-04-project-format-migration-architecture](2026-08-04-project-format-migration-architecture.md)
  ("pull → migrate in the library → push, never in place") for one case: an
  update rewrites the board's own stored projects in place, written by the
  host. The roster's library-copy Upgrade verb is unchanged.
- **Superseded by:** None
- **Related:** [2026-10-28-project-format-policy](2026-10-28-project-format-policy.md)
  (the policy this path serves),
  [2026-10-06-studio-updates-over-the-update-channel](2026-10-06-studio-updates-over-the-update-channel.md)
  (the update this stage ends; amended 2026-10-28),
  [2026-10-06-ota-update-protocol](2026-10-06-ota-update-protocol.md)
  (channel 3, which this stage does not touch),
  [2026-10-06-firmware-distribution](2026-10-06-firmware-distribution.md)
  (the release index's additive `projectFormat`),
  [2026-10-07-project-loads-are-tried-and-recovered](2026-10-07-project-loads-are-tried-and-recovered.md)
  (a refused load leaves the startup choice alone),
  [2026-10-27-mapping-model-parts-segments-patcher](2026-10-27-mapping-model-parts-segments-patcher.md)
  (project format 12, the first bump to cross this path).
  Code: `lp-app/lpa-update/README.md` ("The project stage").
- **Plan:** `lp2025/2026-10-27-0114-mapping-rebuild` (M3, phases P10–P12;
  `notes.md` "M3 amended for G1", Q99–Q114).

## Context

Project format 12 (the mapping rebuild: map2d format 6 holds the patching)
is the first project-format bump since boards update over the air. Until
now a bump meant: the library upgrades on open, and a board holding an old
project refuses it until someone pushes an upgraded copy. With boards in
houses that take their firmware from lightplayer.app, that leaves every
updated board dark until a person notices and acts.

The earlier lean kept that shape: the updated board refuses its old
projects, and Studio offers an Upgrade button. **Yona reversed it at the
format gate (G1, item 9):** upgrading the board's stored projects is part of
the firmware update, driven by the client (Studio or lp-cli, through the
shared update host), and automatic. "It's the cost of updating things… we
have all the machinery for it."

Two facts decide where in the update it can happen:

- **The engine that reads projects changes at exactly one point: engine Y's
  first start.** Only the engine reads projects (the core reads the access
  file, the network file and the status light). A split image's engine never
  runs on an unconfirmed trial, and once core Y is confirmed the only engine
  that can run is the one its digest slot names. Every path before engine
  Y's first start — a cut in the backup, the core or the engine, a failed
  trial and its rollback, a heal — comes back to engine X, which reads the
  old format, or to core-only, which reads nothing. After it, only Y runs.
- **A fielded board's files are out of a newer host's reach until its new
  engine speaks the host's wire.** Studio's hello gate refuses any board on
  another `WIRE_PROTO_VERSION`, and the wire bumps often, so channel 1 is
  closed to an older board: the host speaks only channel 3 to it. Channel 3
  is frozen and additive, and a fielded core X cannot learn new messages on
  it. So no host can read or write the board's projects before the update;
  the first moment it can is when engine Y answers on the host's own wire.

Channel 1 already has every verb the upgrade needs (`FsRequest::{Read,
ListDir, Write, DeleteFile, BeginBatch, CommitBatch, AbortBatch}`,
`ListAvailableProjects`, `LoadProject`), and on the tree store (the C6's and
the S3's filesystem, the two chips that update over the air) a batch is one
transaction: a cut before its commit leaves the old state.

## Decision

1. **The update's last stage upgrades the board's projects, over channel 1,
   driven by the host.** Once engine Y answers, the host lists the board's
   projects, reads each `project.json`'s format, and for every project older
   than the format Y reads: reads its files, runs `lpa_upgrade::upgrade_to`
   (the target is Y's format), and writes the result back as **one atomic
   tree-store batch per project** (changed and added files written, removed
   ones deleted, then commit). The startup project (named by
   `/lightplayer.json`, else the first by path, as boot does) goes first and
   is loaded (`LoadProject`) as soon as its own batch commits, so the board
   lights before the others follow. The update reports up to date only after
   the stage.
2. **It resumes from the board's own bytes, never from memory.** Every run
   lists and reads afresh; a project already at Y's format is left alone.
   After a cut each project is wholly old or wholly new, and the old bytes
   the step needs are still on the board, so any later run finishes it.
   If a run is interrupted after the startup project commits but before its
   load, the next run loads it only when the board's hello says it refused
   that project for its format (`startupRefused`, below), never otherwise,
   so a board someone stopped on purpose stays stopped.
3. **One shared, sans-IO stage; one edge.** `lpa-update`'s `ProjectStage`
   (`src/projects/`), a sibling of `UpdateDriver` (which is untouched), takes
   the engine's format and the link's tier, and emits listings, reads,
   batches, the load and a report. `lpa_client::run_project_stage` is the
   one channel-1 edge, called by Studio and lp-cli alike. The stage is
   general: it knows formats and `lpa-upgrade`'s chain, nothing of what a
   step changes.
4. **The host learns the formats from the board.** The hello carries the
   engine's `PROJECT_FORMAT_VERSION` as `build.projectFormat` (one
   `WIRE_PROTO_VERSION` bump, to 46); each stored project's format is read
   from its `project.json`. The hello also carries `startupRefused`
   (`{project, found}`, absent when nothing was refused): set when boot
   refused the startup project for its format, cleared by any load, unload
   or stop. Released firmware says its project format too: an additive
   `projectFormat` in `ota-manifest.json` and the release index (still
   format 1), so Studio can say before the press that a version upgrades the
   board's projects, or strands them on an older version. Releases without
   the field fall back to a guess from their wire version.
5. **Edit tier.** Writing files is the edit tier, the tier an install
   already needs. On a cable the link is edit; wirelessly it is the key's
   granted tier. A play or anonymous link writes nothing, and the card says:
   "Its projects need upgrading for this firmware. Someone who can edit this
   board can finish it." The heal rule (no login) is unchanged.
6. **A refused project stays as it was and is named.** `upgrade_to` is
   all-or-nothing per project. The others are upgraded; the refused one is
   left untouched and named with the step's own sentence ("Couldn't upgrade
   “quad”: …"). No escape verb.
7. **A host too old says so.** When the engine reads a format past this
   host's chain, the stage reads nothing and writes nothing, and says to
   update lp-cli or reload Studio to finish the board. Any current host
   finishes it.
8. **A downgrade across a bump is already Lasting** ("Other version…" to
   an older release). Its copy adds that the board's projects were saved for
   newer firmware and won't load on that version. Nothing else.
9. **It starts itself, with no click.** After any update or flash, and
   whenever Studio finds a board on its own wire whose engine reads a newer
   format than a project it stores, the stage runs on an edit link
   (Routine: a project the engine refuses loses nothing by being upgraded,
   and the step is value-preserving). The card says what it did ("Upgrading
   projects · 1 of 2", then a journal line naming the projects upgraded).
   `lp-cli firmware install` runs it when the board comes back speaking its
   wire, and `lp-cli emu run --ota-offer` after the update; the `just
   flash-*` dev recipes do not.
10. **The library copy is untouched.** The update rewrites the board's
    copy, not Studio's library. The step is deterministic, so a project
    upgraded on the board and its library copy upgraded on open hash the
    same.
11. **Studio refuses to push a newer-format project to an older board**
    before touching it: "This board's firmware reads projects up to format
    11. Update it to push this project."
12. **The device never migrates a file.** Firmware refuses an older project
    by name and keeps serving files; only a host rewrites them.

## Consequences

- **The one window.** Between engine Y's first start and the commit of a
  project's batch, the running engine holds a project it cannot read. It
  refuses it by name and lights nothing, as a board waiting for its engine
  already does for a whole transfer; on USB and the LAN this lasts seconds.
  A power cut inside it leaves the board that way until a host with edit
  reaches it, which then finishes with no click. Closing it would mean
  writing before Y starts, which only a new channel-3 file service in core Y
  could do: core flash near its 32 KiB page cliff, a permanent addition to
  the update protocol (old cores answering "unknown"), and the core's own
  access check on writes. Not done; revisit if a desk or field walk shows a
  lamp left dark.
- **Every later project-format bump inherits this path with no new
  machinery.** `upgrade_to(target)` and the stage know nothing about
  mapping; a test proves a toy v12→v13 step rides the same stage.
- **Cost on the C6:** the stage is host code. The board's part is the hello
  fields and the boot's refusal report: core +16 B, engine about +1.6 KB.
- **Known limits, shipped as accepted:** lp-cli's `link capture --ota-offer`
  and lp-cli through `relay:` do not run the stage (Studio does, over every
  link); the play-tier path is proven at the stage and its words, not end
  to end through a play login; Studio waits at most 10 s for a wireless
  link's tier before ending the update with a line, after which the no-click
  stage runs once the link holds one. A board whose store has no
  transactions (littlefs, which never updates over the air) gets nothing
  written.
- **Walked:** see `docs/reports/2026-10-28-ota-project-upgrade-walk.md`.
  The stage ran on USB, the LAN, the relay and Bluetooth, and power cuts
  at every flash operation of it were finished by the next host. **The
  walk found that the update before it reaches a board on an older wire
  only over a cable**: Studio updates such a board with a USB flash, and
  over Wi‑Fi, Bluetooth and the relay it offers none ("Firmware updates
  need USB"). Every wire bump does this; whether the update channel should
  carry an older-wire board's update is open (the walk record's "Found").

## Alternatives Considered

- **A core-only file service between trial confirm and engine install.**
  The only strict placement that never leaves a running engine with a
  project it cannot read. Rejected for now for the cost above (core flash,
  a forever channel-3 addition).
- **Staging the upgraded files early, before the update.** Impossible on a
  fielded board: nothing reaches its files before engine Y speaks the
  host's wire.
- **Refusing and offering an Upgrade button** (the lean before G1). Leaves
  every updated board dark until a person acts, and a board updated from
  lp-cli or another browser may have no library copy to push from.
- **Migrating on the device.** Rejected by the policy: migrations live in
  hosts, and an upgrader in firmware costs flash and RAM for a job a host
  can do.

## Follow-ups

- Close the window with a core file service, only if a walk shows a lamp
  left dark (indexed in `docs/adr/README.md`'s deferred table).
- lp-cli's `link capture --ota-offer` and relay host: run the stage when
  either becomes a route people update through.
- An end-to-end play-tier statement once the bench can log in at play.
