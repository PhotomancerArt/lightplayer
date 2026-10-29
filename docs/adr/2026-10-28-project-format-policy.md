# ADR: Project-format policy — change is the heartbeat, migrations live in hosts

- **Status:** Accepted (Yona, 2026-10-27)
- **Date:** 2026-10-28
- **Deciders:** Yona
- **Supersedes:** None
- **Superseded by:** None
- **Related:** [2026-10-28-firmware-updates-upgrade-the-boards-projects](2026-10-28-firmware-updates-upgrade-the-boards-projects.md)
  (how a board's projects are upgraded),
  [2026-08-04-project-format-migration-architecture](2026-08-04-project-format-migration-architecture.md)
  (`lpa-upgrade`, its chain and its goldens). The drill is AGENTS.md
  "Persisted-format compatibility" (`just format-bump`).
- **Plan:** `lp2025/2026-10-27-0114-mapping-rebuild` (`notes.md` "Format
  changes: Yona's policy and the queued second bump").

## Context

The project format keeps moving: format 12 (the mapping rebuild) is the
first bump to reach boards that update over the air, and another (playlist
triggers, v12→v13) is queued behind it. Each bump so far carried the same
questions: keep the old shape readable or not, where the migration runs,
and whether a bump needs Yona.

## Decision

- **Change is the heartbeat.** Format debt is paid in migration steps, one
  per upgrade path, each with tests (`lpa-upgrade`, its corpus and goldens),
  never by carrying legacy shapes in the data model.
- **One current shape.** The model reads and writes the current format only:
  no serde aliases, no dual reads, no legacy shapes kept beside the new one.
- **Migrations live in hosts** (Studio and lp-cli, through `lpa-upgrade`),
  never long-term in firmware. A board upgrades only by a host rewriting
  its files.
- **Migrations are applied automatically**: when Studio opens a library
  project, and during a firmware update or flash, which upgrades the board's
  stored projects as its last stage
  ([2026-10-28-firmware-updates-upgrade-the-boards-projects](2026-10-28-firmware-updates-upgrade-the-boards-projects.md)).
- **Compatibility tiers, strictest first:**
  1. **Over-the-air updates:** backward compatible for a long while (the
     never-break pins in AGENTS.md "Wire/protocol compatibility").
  2. **Basic comms:** what is needed to show the device card and update.
  3. **The Panel:** ideally not broken by ordinary settings changes.
  4. **Studio:** needing a firmware update before modifying a project is
     fine.
- **A project-format change stays an ask, but a cheap one**, asked in one
  line: "bump for X; the step does Y; the corpus shows behaviour unchanged."
  It stays an ask because a revert cannot un-upgrade files already
  rewritten, on boards and in libraries.

## Consequences

- A bump is a step, its tests, the corpus re-blessed and the repo's
  projects migrated in the same change (`just format-bump`, then the drill
  in AGENTS.md). It needs no new device machinery: the update's project
  stage runs any step in the chain.
- Firmware stays small and current-only; an older project on a board is
  refused by name until a host upgrades it.
- Old shapes leave the code with the bump that replaces them.

## Alternatives Considered

- **Reading old shapes in the model** (aliases, dual reads): rejected; it
  grows every reader and keeps dead formats alive.
- **Migrating on the device:** rejected; it costs flash and RAM for a job a
  host already does, and every old step would ride in every image.
- **Bumps without asking:** rejected; a rewritten file cannot be reverted.

## Follow-ups

- After the mapping rebuild, a separate plan slims the bump ritual
  (`just format-bump`, corpus copy, schema snapshot, catalog and projects
  migration, golden re-bless).
