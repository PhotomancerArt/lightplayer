# Phase 4: Add HostSpecifier::Emulator Variant

## Scope of Phase

Add `HostSpecifier::Emulator` variant to support `--push emu` in lp-cli. This includes parsing and display support.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

1. Update `lp-core/lp-client/src/specifier.rs`:

   Add `Emulator` variant to `HostSpecifier` enum:
   ```rust
   pub enum HostSpecifier {
       WebSocket { url: String },
       Serial { port: Option<String> },
       Local,
       Emulator,  // NEW
   }
   ```

2. Update `HostSpecifier::parse()`:
   - Add check for "emu" or "emulator" strings:
     ```rust
     if s == "emu" || s == "emulator" {
         return Ok(HostSpecifier::Emulator);
     }
     ```
   - Update error message to include "emu" or "emulator"

3. Update `Display` implementation:
   - Add case for `HostSpecifier::Emulator` → `"emu"`

4. Add helper method:
   ```rust
   pub fn is_emulator(&self) -> bool {
       matches!(self, HostSpecifier::Emulator)
   }
   ```

5. Update tests in `lp-core/lp-client/src/specifier.rs`:
   - Test parsing "emu" → `HostSpecifier::Emulator`
   - Test parsing "emulator" → `HostSpecifier::Emulator`
   - Test display of `HostSpecifier::Emulator` → `"emu"`
   - Test `is_emulator()` method

## Tests

All tests should be in the existing test module in `specifier.rs`.

## Validate

Run: `cd lp-core/lp-client && cargo test specifier`

Fix any warnings or errors. Keep code compiling.
