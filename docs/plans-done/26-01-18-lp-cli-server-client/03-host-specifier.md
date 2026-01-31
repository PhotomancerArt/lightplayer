# Phase 3: Implement Host Specifier Parsing

## Goal

Implement parsing of host specifiers (ws://, serial:) to determine transport type and parameters.

## Tasks

1. Create `lp-app/apps/lp-cli/src/transport/mod.rs`:
   - Re-export `specifier` and `websocket` modules

2. Create `lp-app/apps/lp-cli/src/transport/specifier.rs`:
   - Define `HostSpecifier` enum:
     ```rust
     pub enum HostSpecifier {
         WebSocket { url: String },
         Serial { port: Option<String> }, // None = auto
     }
     ```
   - Implement `HostSpecifier::parse(s: &str) -> Result<Self, Error>`:
     - Parse `ws://` or `wss://` URLs → `WebSocket { url }`
     - Parse `serial:` prefix → `Serial { port }` (strip prefix, None if "auto")
     - Return clear error for invalid formats
   - Implement `Display` trait for `HostSpecifier`
   - Add helper methods:
     - `is_websocket(&self) -> bool`
     - `is_serial(&self) -> bool`

3. Add tests for host specifier parsing:
   - Test valid websocket URLs (`ws://localhost:2812/`, `wss://example.com/`)
   - Test valid serial specifiers (`serial:auto`, `serial:/dev/ttyUSB1`)
   - Test invalid formats (clear error messages)
   - Test edge cases (trailing slashes, etc.)

## Success Criteria

- Host specifiers can be parsed from strings
- Clear error messages for invalid formats
- Supports websocket and serial formats
- Tests cover all cases
- Code compiles without warnings
