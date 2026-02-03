# Phase 1: Create lp-client crate structure

## Scope of phase

Create the basic structure for the new `lp-client` crate, including directory structure, `Cargo.toml` with dependencies, and a placeholder `lib.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. Create directory structure:

   ```
   lp-core/lp-client/
   ├── Cargo.toml
   └── src/
       └── lib.rs
   ```

2. Create `Cargo.toml`:
   - Use workspace version, authors, edition, license, rust-version
   - Dependencies:
     - `lp-model` (path dependency)
     - `lp-shared` (path dependency)
     - `tokio` (with "full" features)
     - `anyhow`
     - `serde_json`
     - `tokio-tungstenite` (version "0.21")
     - `futures-util` (version "0.3")
     - `async-trait` (version "0.1")
     - `serde` (with "derive" feature)

3. Create placeholder `src/lib.rs`:

   ```rust
   //! LightPlayer client library.
   //!
   //! Provides client-side functionality for communicating with LpServer.
   //! Includes transport implementations and the main LpClient struct.

   // Placeholder - will be populated in Phase 3
   ```

4. Add `lp-client` to workspace members in root `Cargo.toml`:
   - Add `"lp-core/lp-client"` to the `members` array
   - Add `"lp-core/lp-client"` to the `default-members` array

## Validate

Run `cargo check --package lp-client` to verify the crate structure is valid.
