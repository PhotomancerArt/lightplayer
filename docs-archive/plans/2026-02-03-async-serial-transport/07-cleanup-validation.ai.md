# Phase 7: Cleanup & Validation

## Cleanup & validation

Grep the git diff for any temporary code, TODOs, debug prints, etc. Remove them.

Run: `cd lp-core/lp-client && cargo check --tests && cargo test`
Run: `cd lp-cli && cargo check --tests && cargo test`

Fix all warnings, errors, and formatting issues.

Run: `cargo +nightly fmt` on all changed files.

## Plan cleanup

Add a summary of the completed work to `docs/plans/2026-02-03-async-serial-transport/summary.md`.

Move the plan files to the `docs/plans-done/` directory.

## Commit

Once the plan is complete, and everything compiles and passes tests, commit the changes with a message following the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
feat(client): add async serial transport for emulator

- Add generic AsyncSerialClientTransport that uses channels
- Add create_emulator_serial_transport_pair() factory function
- Implement emulator thread loop for continuous execution
- Add HostSpecifier::Emulator variant for lp-cli integration
- Integrate with lp-cli client_connect for --push emu support
- Add async test scene_render_emu_async.rs

The transport is generic and can be reused for hardware serial
in the future. Only the factory function knows about emulator
implementation details.
```
