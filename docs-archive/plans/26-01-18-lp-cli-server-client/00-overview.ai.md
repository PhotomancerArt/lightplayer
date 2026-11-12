# Plan: LP-CLI Server and Client Modes

## Overview

Implement server and client modes for the `lp-cli` application, enabling local development and testing of the lightplayer client-server architecture. This includes:

- Server mode: Run server from a directory with websocket API
- Client mode: Connect to server and sync local projects
- Create command: Initialize new projects with sensible defaults
- WebSocket transport implementations for both client and server
- User-friendly messaging with actionable next steps

This plan builds on existing `lp-server` and `lp-client` libraries and adds CLI interfaces, transport implementations, and project management workflows.

## Phases

1. **Set up CLI structure and dependencies** - Add clap, anyhow, basic command structure
2. **Create ServerConfig and server.json handling** - ServerConfig struct, serialization, validation
3. **Implement host specifier parsing** - Parse ws://, serial: formats
4. **Implement websocket client transport** - Sync tungstenite client transport
5. **Implement websocket server transport** - Async tokio-tungstenite server transport
6. **Implement create command** - Project creation with defaults
7. **Implement serve command** - Server startup, initialization, main loop
8. **Implement dev command and project push logic** - Client connection, project push, file sync
9. **Add user-friendly messaging helpers** - Success/error message formatting
10. **Add integration tests** - Memory filesystem, in-memory transport tests
11. **Cleanup and finalization** - Fix warnings, tests, documentation

## Success Criteria

- `lp-cli serve <dir>` starts server and accepts websocket connections
- `lp-cli serve --init` creates server.json if missing
- `lp-cli serve --memory` runs with in-memory filesystem
- `lp-cli dev <host> <dir>` connects and pushes local project
- `lp-cli create <dir>` creates new project structure with sensible defaults
- Server handles multiple client connections
- Client can push project and server loads it correctly
- All code is testable with memory filesystem and in-memory transport
- All tests pass
- Code compiles without warnings
