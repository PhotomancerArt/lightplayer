# Phase 12: Update fw-host CLI (--create flag, project dir argument)

## Goal

Add CLI arguments to `fw-host` for project directory and `--create` flag.

## Tasks

1. Add CLI argument parsing to `fw-host/src/main.rs`:
   - Use `clap` or similar (or manual parsing)
   - `--project-dir <path>` - specify project directory (optional, defaults to in-memory)
   - `--create` - create new project structure in specified directory

2. Implement `--create` functionality:
   - Create project directory structure:
     - `project.json` with default config
     - `src/` directory
   - Optionally create example nodes (or leave empty)

3. Update project loading logic:
   - If `--project-dir` specified, use `HostFilesystem` with that path
   - If not specified, use `InMemoryFilesystem` with sample project (testing mode)

4. Update filesystem watcher:
   - Only create watcher if using real filesystem (not in-memory)

5. Add help text and error messages

## Success Criteria

- CLI accepts `--project-dir` and `--create` flags
- `--create` creates project structure
- Default behavior uses in-memory filesystem (testing mode)
- Code compiles without warnings
- Help text is clear

