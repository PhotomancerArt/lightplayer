# Phase 9: Add User-Friendly Messaging Helpers

## Goal

Implement user-friendly messaging helpers that provide actionable next steps and copy-pasteable commands.

## Tasks

1. Create `lp-app/apps/lp-cli/src/messages.rs`:
   - `pub fn print_success(message: &str, next_steps: &[&str])`:
     - Print success message with ✓ prefix
     - Print "Next steps:" section
     - Format commands for easy copy-paste
   - `pub fn print_error(message: &str, suggestions: &[&str])`:
     - Print error message with ✗ prefix
     - Print suggestions section
     - Format commands for easy copy-paste
   - `pub fn format_command(cmd: &str) -> String`:
     - Format command with proper quoting if needed
     - Ensure commands are copy-pasteable

2. Update `lp-app/apps/lp-cli/src/commands/create/project.rs`:
   - Use `print_success()` for success message
   - Show: `cd <name> && lp-cli dev ws://localhost:2812/`

3. Update `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - Use `print_error()` when project.json missing
   - Show: `lp-cli create .` or `cd /path/to/project && lp-cli dev ...`

4. Update `lp-app/apps/lp-cli/src/commands/serve/init.rs`:
   - Use `print_error()` when server.json missing
   - Show: `lp-cli serve <dir> --init`

5. Add tests:
   - Test message formatting
   - Test command formatting
   - Verify output format

## Success Criteria

- Success messages show next steps
- Error messages show exact commands to fix issues
- Commands are copy-pasteable
- Consistent formatting (✓ for success, ✗ for errors)
- Tests pass
- Code compiles without warnings
