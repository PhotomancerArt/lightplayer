# Phase 2: Create ServerConfig and server.json Handling

## Goal

Create `ServerConfig` struct and implement server.json file handling for server initialization.

## Tasks

1. Create `lp-app/crates/lp-model/src/server/config.rs`:
   - Define `ServerConfig` struct:
     ```rust
     #[derive(Debug, Clone, Serialize, Deserialize)]
     pub struct ServerConfig {
         // Future: memory_limits, security_rules, etc.
         // For now, empty struct serializes to {}
     }
     ```
   - Add comments documenting future fields
   - Implement `Default` trait (returns empty struct)

2. Update `lp-app/crates/lp-model/src/server/mod.rs`:
   - Export `config` module

3. Create `lp-app/apps/lp-cli/src/config/mod.rs`:
   - Re-export server config

4. Create `lp-app/apps/lp-cli/src/config/server.rs`:
   - `pub fn load_server_config(dir: &Path) -> Result<ServerConfig>` - Load server.json
   - `pub fn save_server_config(dir: &Path, config: &ServerConfig) -> Result<()>` - Save server.json
   - `pub fn server_config_exists(dir: &Path) -> bool` - Check if server.json exists
   - Handle file I/O errors with context

5. Add tests for `ServerConfig`:
   - Test serialization/deserialization (empty JSON)
   - Test file loading/saving

## Success Criteria

- `ServerConfig` struct exists in `lp-model`
- Can serialize/deserialize to/from JSON (`{}`)
- Can load and save server.json files
- File operations provide clear error messages
- Tests pass
- Code compiles without warnings
