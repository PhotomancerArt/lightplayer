# Phase 1: Add lp-cli upload command

## Scope of phase

Add `lp-cli upload <dir> <host>` command that connects, stops all projects, pushes project, loads project, and exits. Non-interactive.

## Implementation Details

- Create `lp-cli/src/commands/upload/` with args.rs, handler.rs, mod.rs
- Extract `validate_local_project` to `dev/validation.rs` for reuse
- Add Upload subcommand to main.rs Cli enum
- Handler: resolve dir, validate, parse host, connect, stop_all, push_project_async, project_load, exit

## Validate

```bash
cargo build -p lp-cli
cargo run -p lp-cli -- upload --help
```
