# Plan: Dev Command In-Memory Server

## Overview

Enable `lp-cli dev` to run an in-memory server by default when no host is specified, eliminating the need to run separate `serve` and `dev` commands for development and testing. The in-memory server uses a local transport with tokio channels for communication between server and client running in the same process.

## Phases

1. **Add HostSpecifier::Local variant** - Add local variant to HostSpecifier enum and update parsing logic
2. **Create AsyncLocalTransport** - Implement async-capable local transport using tokio channels
3. **Refactor server initialization** - Extract server creation logic to shared module
4. **Convert server loop to async** - Make server loop async-compatible for tokio runtime
5. **Update dev command for in-memory server** - Modify dev command to support local transport and in-memory server
6. **Add Ctrl+C handling and client loop** - Implement graceful shutdown and continuous client loop
7. **Cleanup and tests** - Remove temporary code, add tests, fix warnings

## Success Criteria

- `lp-cli dev` runs without requiring a host parameter
- In-memory server starts automatically when no host is specified
- Server and client run in the same process using local transport
- Both server and client loops run continuously until Ctrl+C
- Code is properly refactored to minimize duplication with `serve` command
- All tests pass
- Code compiles without warnings
