# Phase 4: Cleanup And Validation

## Scope Of Phase

In scope:

- remove temporary test helpers that became unused
- ensure comments and test names describe the SlotCodec boundary clearly
- update roadmap notes if Phase 3 leaves a transport follow-up
- run focused validation

Out of scope:

- removing serde derives
- switching definition loading
- changing schema snapshot serialization

## Code Organization Reminders

- Keep tests at the bottom of files.
- Do not leave commented-out experiments.
- Prefer precise helper names over long explanatory comments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Final acceptance criteria:

- project-read detailed node slot payloads are written through SlotCodec
- tests read those payloads back through the slot registry
- no production detailed node slot writer path builds `SlotData` just to write
  JSON
- any remaining serde usage in M2 files is explicitly outside the model slot
  payload boundary

## Validate

```bash
cargo test -p lpc-engine project_read
cargo test -p lpc-wire project_read
cargo test -p lpc-model slot_codec
```

If transport fallback changed:

```bash
cargo test -p lpc-shared
cargo test -p lpa-server --no-run
```
