# Phase 2: Create Test Message Types and Protocol

## Scope of phase

Define test command and response types using external discriminators for serde-json-core. Implement JSON serialization/deserialization and message parsing with `M!` prefix support.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create test message types

Create `lp-fw/fw-core/src/test_messages.rs`:

```rust
//! Test message types for USB serial testing
//!
//! Defines command and response types for test protocol using external
//! discriminators compatible with serde-json-core.

extern crate alloc;

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Test command (external discriminator format)
///
/// Commands use verb-based names and external discriminators:
/// - `M!{"get_frame_count":{}}\n`
/// - `M!{"echo":{"data":"test"}}\n`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum TestCommand {
    /// Get current frame count
    #[serde(rename = "get_frame_count")]
    GetFrameCount,
    
    /// Echo a message back
    #[serde(rename = "echo")]
    Echo {
        /// Data to echo back
        data: String,
    },
}

/// Test response (external discriminator format)
///
/// Responses match command structure:
/// - `M!{"frame_count":12345}\n`
/// - `M!{"echo":"test"}\n`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum TestResponse {
    /// Frame count response
    FrameCount {
        /// Current frame count
        frame_count: u32,
    },
    
    /// Echo response
    Echo {
        /// Echoed data
        echo: String,
    },
}

/// Parse a message line with M! prefix
///
/// Extracts the JSON portion after the `M!` prefix and newline.
///
/// # Arguments
///
/// * `line` - Line to parse (should include `M!` prefix and `\n`)
///
/// # Returns
///
/// * `Some(json_str)` if line starts with `M!` and contains valid JSON
/// * `None` if line doesn't start with `M!` or is invalid
pub fn parse_message_line(line: &str) -> Option<&str> {
    // Remove trailing newline/carriage return
    let line = line.trim_end_matches('\n').trim_end_matches('\r');
    
    // Check for M! prefix
    if !line.starts_with("M!") {
        return None;
    }
    
    // Extract JSON portion (after "M!")
    Some(&line[2..])
}

/// Serialize a test command to message format
///
/// Formats command as `M!{json}\n` for transmission.
///
/// # Arguments
///
/// * `cmd` - Command to serialize
///
/// # Returns
///
/// Serialized message string with `M!` prefix and newline
pub fn serialize_command(cmd: &TestCommand) -> Result<String, serde_json_core::ser::Error> {
    use lp_model::json;
    
    let json = json::to_string(cmd)?;
    Ok(format!("M!{}\n", json))
}

/// Deserialize a test command from message format
///
/// Parses `M!{json}\n` format and extracts command.
///
/// # Arguments
///
/// * `line` - Message line to parse
///
/// # Returns
///
/// * `Ok(Some(cmd))` if valid command
/// * `Ok(None)` if line doesn't start with `M!` (not a message)
/// * `Err` if JSON parsing fails
pub fn deserialize_command(line: &str) -> Result<Option<TestCommand>, serde_json_core::de::Error> {
    let json_str = match parse_message_line(line) {
        Some(s) => s,
        None => return Ok(None), // Not a message line
    };
    
    use lp_model::json;
    let cmd: TestCommand = json::from_str(json_str)?;
    Ok(Some(cmd))
}

/// Serialize a test response to message format
///
/// Formats response as `M!{json}\n` for transmission.
///
/// # Arguments
///
/// * `resp` - Response to serialize
///
/// # Returns
///
/// Serialized message string with `M!` prefix and newline
pub fn serialize_response(resp: &TestResponse) -> Result<String, serde_json_core::ser::Error> {
    use lp_model::json;
    
    let json = json::to_string(resp)?;
    Ok(format!("M!{}\n", json))
}
```

### 2. Export from lib.rs

Update `lp-fw/fw-core/src/lib.rs`:
```rust
// ... existing code ...

pub mod test_messages;

pub use test_messages::{TestCommand, TestResponse, parse_message_line, serialize_command, deserialize_command, serialize_response};
```

### 3. Add tests

Add tests to `lp-fw/fw-core/src/test_messages.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_get_frame_count() {
        let cmd = TestCommand::GetFrameCount;
        let msg = serialize_command(&cmd).unwrap();
        assert!(msg.starts_with("M!"));
        assert!(msg.ends_with('\n'));
        assert!(msg.contains("get_frame_count"));
    }

    #[test]
    fn test_serialize_echo() {
        let cmd = TestCommand::Echo {
            data: "test".to_string(),
        };
        let msg = serialize_command(&cmd).unwrap();
        assert!(msg.starts_with("M!"));
        assert!(msg.ends_with('\n'));
        assert!(msg.contains("echo"));
        assert!(msg.contains("test"));
    }

    #[test]
    fn test_deserialize_get_frame_count() {
        let line = "M!{\"get_frame_count\":{}}\n";
        let cmd = deserialize_command(line).unwrap().unwrap();
        assert!(matches!(cmd, TestCommand::GetFrameCount));
    }

    #[test]
    fn test_deserialize_echo() {
        let line = "M!{\"echo\":{\"data\":\"test\"}}\n";
        let cmd = deserialize_command(line).unwrap().unwrap();
        match cmd {
            TestCommand::Echo { data } => assert_eq!(data, "test"),
            _ => panic!("Expected Echo command"),
        }
    }

    #[test]
    fn test_parse_message_line() {
        assert_eq!(parse_message_line("M!{\"test\":1}\n"), Some("{\"test\":1}"));
        assert_eq!(parse_message_line("M!{\"test\":1}"), Some("{\"test\":1}"));
        assert_eq!(parse_message_line("debug output\n"), None);
        assert_eq!(parse_message_line("not a message"), None);
    }

    #[test]
    fn test_serialize_frame_count_response() {
        let resp = TestResponse::FrameCount { frame_count: 12345 };
        let msg = serialize_response(&resp).unwrap();
        assert!(msg.starts_with("M!"));
        assert!(msg.ends_with('\n'));
        assert!(msg.contains("12345"));
    }

    #[test]
    fn test_serialize_echo_response() {
        let resp = TestResponse::Echo {
            echo: "test".to_string(),
        };
        let msg = serialize_response(&resp).unwrap();
        assert!(msg.starts_with("M!"));
        assert!(msg.ends_with('\n'));
        assert!(msg.contains("test"));
    }

    #[test]
    fn test_round_trip_command() {
        let original = TestCommand::Echo {
            data: "round trip".to_string(),
        };
        let serialized = serialize_command(&original).unwrap();
        let deserialized = deserialize_command(&serialized).unwrap().unwrap();
        assert_eq!(original, deserialized);
    }
}
```

## Tests to Write

- Test serialization of `GetFrameCount` command
- Test serialization of `Echo` command
- Test deserialization of commands
- Test `parse_message_line()` with various inputs
- Test serialization of responses
- Test round-trip serialization/deserialization
- Test that non-M! lines return `None` (not a message)

## Validate

Run from `lp-fw/fw-core/` directory:

```bash
cd lp-fw/fw-core
cargo test --package fw-core
cargo check --package fw-core
```

Ensure:
- All tests pass
- No warnings
- Code compiles for both `std` and `no_std` targets
- Message types are exported from `fw-core`
- JSON format matches expected external discriminator format
