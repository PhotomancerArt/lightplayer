# Phase 1: Add HostSpecifier::Local Variant

## Description

Add a `Local` variant to the `HostSpecifier` enum and update parsing logic to handle the case when no host is specified or when "local" is explicitly provided. This enables the `dev` command to detect when it should use an in-memory server.

## Tasks

1. Update `lp-app/apps/lp-cli/src/transport/specifier.rs`:
   - Add `Local` variant to `HostSpecifier` enum
   - Update `parse()` method to handle empty string or "local" as `Local`
   - Add `parse_optional()` method to handle `Option<&str>`:
     ```rust
     pub fn parse_optional(s: Option<&str>) -> Result<Self> {
         match s {
             None | Some("") | Some("local") => Ok(HostSpecifier::Local),
             Some(s) => Self::parse(s),
         }
     }
     ```
   - Update `Display` implementation to handle `Local` variant
   - Update `is_websocket()` and `is_serial()` methods (add `is_local()` if needed)

2. Update tests in `specifier.rs`:
   - Test parsing `None` as `Local`
   - Test parsing empty string as `Local`
   - Test parsing "local" as `Local`
   - Test `parse_optional()` with various inputs
   - Test `Display` for `Local` variant

## Success Criteria

- `HostSpecifier::Local` variant exists
- `parse_optional()` correctly handles `None`/empty/"local" as `Local`
- Existing parsing logic still works for websocket and serial
- All tests pass
- Code compiles without warnings

## Implementation Notes

- Keep existing `parse()` method unchanged for backward compatibility
- `parse_optional()` is a convenience method for handling optional host strings
- Display format for `Local` can be `"local"` or similar
