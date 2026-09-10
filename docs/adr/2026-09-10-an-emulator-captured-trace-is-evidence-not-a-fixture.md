# ADR: An emulator-captured device trace is evidence, not a golden fixture

- **Status:** **Accepted** (emulator plan two, M6; OQ3 — ratified by Yona at
  G2, 2026-09-10).
- **Date:** 2026-09-10
- **Deciders:** Photomancer
- **Amends:** the golden-trace library described in
  `lp-app/lpa-link/testdata/device-traces/README.md` and
  `scripts/device-scenarios/README.md`
- **Supersedes:** None

## Context

`lp-app/lpa-link/testdata/device-traces/` holds nine committed `.jsonl`
traces, each one a recording of what Studio observed while a **real board**
did something. Three consumers read them: `lp-app/lpa-link/tests/trace_replay.rs`
replays their raw lines through the boot classifier, the virtual device's
fidelity gate diffs against them, and live-jank diagnosis compares a
misbehaving session against the capture for its scenario.

Emulator plan two makes it possible to produce the same kind of trace with no
board at all (`just device-scenario run <id> --emu`). So the directory is
about to hold two kinds of file that look identical, and the question — the
plan's OQ3 — is where the emulated ones live and what tells them apart.

The plan's own lean was *"alongside, distinguished by a sidecar
`configuration` field mirroring vision D18's `silicon:` / `lp-emu:` naming."*
**That mechanism does not exist.** The device-trace directory has no sidecar
of any kind and no `configuration` field anywhere; the `configuration` field
the lean remembers belongs to the *other* trace system,
`lp-emu/transcripts/<chip>/<payload>/*.txt.meta.json`.

## Decision

**1. An emulator-captured trace is named `<id>.emu.jsonl`** (or
`<id>.emu.failed.jsonl` for a finding), beside the silicon `<id>.jsonl`, in
the same directory.

**2. The emulated lane may only ever write a name containing `.emu.`** This is
code, not convention — `assertLaneMayWrite` in `scripts/device-scenario.mjs`,
called at every write site, refusing rather than confirming:

```js
function assertLaneMayWrite(file, lane) {
  const name = path.basename(file);
  if (lane !== "emulated") return file;
  if (!name.includes(".emu.")) {
    throw new Error(
      `the emulated lane tried to write ${name}, which is a SILICON fixture name.\n` +
        `  A board's bytes are not reproducible and a clobbered fixture is gone, so this is\n` +
        `  refused rather than confirmed. Emulated captures are <id>.emu.jsonl (plan two, OQ3).`,
    );
  }
  return file;
}
```

`just device-scenario check-guard` proves it by trying every silicon name and
printing the fixtures' sha256, so "the guard held" is checkable rather than
asserted. The emulated lane also has **no "keep as the golden fixture"
branch** — the silicon lane's escape hatch for "the spec is wrong" simply does
not exist there, because there is no person at a chooser to ask.

**3. Provenance rides inside the file**, as the capture's first record, using
the `journal` kind the contract already has:

```json
{"t":1789041473.711,"kind":"journal","scope":"capture",
 "entry":"emulator-captured trace (plan two M6). configuration=lp-emu:esp32c6:t1 scenario=s1-blank-flash boards=c6-a=blank,kind=rom-up image=… door=… command=just device-scenario run s1-blank-flash --emu. NOT a silicon fixture: no board produced these bytes."}
```

`configuration=lp-emu:esp32c6:t1` is vision D18's naming, the same words the
transcript system's sidecars use. Nothing about the JSONL contract changes:
the kind exists, the fields are its own, no consumer that reads `rx` or
`state` sees it, and no spec's `expect` can match it.

**4. And the rule the other three exist to serve: an emulator-captured trace
may never be the sole evidence for a claim about hardware.** It cannot
overwrite a silicon fixture, it cannot stand in for one that was never
captured, and a classifier claim, a fidelity gate or a defect diagnosis that
rests only on `.emu.` bytes is not established. What an emulated capture *is*
good for is the diff: run beside a silicon fixture, every difference named,
which is a stronger claim than either file makes alone.

## Why this is an ADR and not a naming choice

The test is whether the answer binds future work. Points 1 and 3 alone would
be a filename and a header — a convention. Point 4 is a rule about what
evidence counts, and it reaches every future consumer of that directory: the
virtual device's fidelity gate, plan three's classic-ESP32 scenarios, and
anybody who reaches for `.emu.` bytes when a board is inconvenient. That is
what makes it an ADR.

## Alternatives considered

**(ii) A sidecar** — `<id>.emu.jsonl` plus `<id>.emu.meta.json` carrying
`configuration`, the image, the board id and the capture command, mirroring
the transcript system. **Rejected.** It introduces a second file format into a
directory that has exactly one, and something has to keep the pair in step: a
trace copied out for diagnosis, attached to an issue, or pasted into a report
arrives without its provenance and looks exactly like a silicon fixture. The
transcript system can afford sidecars because its files are opaque text; these
are JSONL with a record contract that already allows additive extension, so
provenance can live inside at no cost.

**(iii) An additive record and nothing else** — provenance inside the file,
same filename as silicon. **Rejected**, and it is the one worth arguing
against carefully, because it is the *most* honest about provenance: the fact
travels with the bytes. It fails on the guard. "The emulated lane may only
write a name containing `.emu.`" is a string test that a reader, a script and
a `git status` can all apply; "the emulated lane may only write a file whose
first record says it is emulated" is a parse, and it cannot protect a file
that already exists from being replaced by one that does. The whole point of
OQ3's non-negotiable half is that overwriting must be *impossible*, not
discouraged — so the discriminator has to be in the name. Point 3 keeps
(iii)'s benefit anyway, by doing both.

## Consequences

- `trace_replay.rs` needs **no change**. It walks `*.jsonl` and skips
  `*.failed.jsonl`, so a passing `<id>.emu.jsonl` is picked up automatically —
  which is a feature and a trap, and the trap is now named: the moment an
  emulator-captured fixture reaches golden status, the classifier is asserting
  on the emulator's own bytes in `just test`. Under point 4 that is acceptable
  *as a regression guard on the emulator*, and it is not evidence about
  silicon.
- **No emulator-captured fixture reaches golden status in M6.** All six of
  this milestone's captures are findings (`<id>.emu.failed.jsonl`), because
  their `expect` lists name record kinds Studio no longer produces
  (`docs/defects/2026-09-10-eight-of-ten-device-event-kinds-lost-their-producer.md`).
  The naming is therefore proven in the guard and in the finding path, and
  not yet in the golden path.
- Plan three's classic-ESP32 scenarios inherit all of this unchanged.
