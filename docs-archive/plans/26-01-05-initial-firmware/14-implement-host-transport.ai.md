# Phase 14: Implement host serial communication (stdio)

## Goal

Implement the transport trait for host using stdio.

## Tasks

1. Create `src/transport.rs`:
   - Implement `lp-core::traits::Transport` trait for host
   - Use `std::io::stdin/stdout` for I/O
   - Implement `send_message()` - write JSON string + `\n` to stdout
   - Implement `receive_message()` - read until `\n` from stdin, return string
   - Handle partial messages (buffer until `\n`)
   - Parse errors should warn and ignore (as specified)
2. Initialize transport in `main.rs`:
   - Create transport instance
3. Test basic message send/receive

## Success Criteria

- Transport trait implementation works
- Can send/receive JSON messages over stdio
- Partial messages are handled correctly
- Parse errors are ignored with warnings
- All code compiles without warnings

