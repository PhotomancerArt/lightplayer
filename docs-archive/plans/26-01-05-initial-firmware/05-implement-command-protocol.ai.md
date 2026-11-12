# Phase 5: Implement command protocol types

## Goal

Implement the command protocol types for JSON message communication.

## Tasks

1. Create `src/protocol/command.rs` with:
   - `Command` enum with `$type` discriminator:
     - `UpdateProject { project: ProjectConfig }`
     - `GetProject`
     - `Log { level: LogLevel, message: String }`
   - `LogLevel` enum: `Info`, `Warn`, `Error`
   - `Response` enum (for future use):
     - `Ok`
     - `Error { message: String }`
2. Implement `serde::Serialize` and `serde::Deserialize` for all types
3. Handle `$type` discriminator field
4. Create `src/protocol/message.rs` with:
   - `Message` trait (extensible for future CRC, framing, etc.)
   - Basic message parsing utilities (read until `\n`, parse JSON)
5. Export from `src/protocol/mod.rs`

## Success Criteria

- Commands can be serialized to/from JSON with `$type` discriminator
- Message parsing utilities work correctly
- All code compiles without warnings

