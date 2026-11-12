# Phase 6: Implement Create Command

## Goal

Implement the `create` command to initialize new projects with sensible defaults.

## Tasks

1. Create `lp-app/apps/lp-cli/src/commands/create/args.rs`:
   - Define `CreateArgs` struct:
     ```rust
     pub struct CreateArgs {
         pub dir: PathBuf,
         pub name: Option<String>,
         pub uid: Option<String>,
     }
     ```
   - Parse from `clap` arguments (name and uid optional)

2. Create `lp-app/apps/lp-cli/src/commands/create/handler.rs`:
   - `pub fn handle_create(args: CreateArgs) -> Result<()>`:
     - Derive name from directory if not provided
     - Call `create_project_structure()`
     - Call `print_success_message()`

3. Create `lp-app/apps/lp-cli/src/commands/create/project.rs`:
   - `pub fn derive_project_name(dir: &Path) -> String`:
     - Extract directory name, sanitize if needed
   - `pub fn generate_uid(name: &str) -> String`:
     - Format: `YYYY.MM.DD-HH.MM.SS-<name>` (use dots, not colons)
     - Example: `2025.01.15-12.15.02-my-project`
   - `pub fn create_project_structure(dir: &Path, name: Option<&str>, uid: Option<&str>) -> Result<()>`:
     - Create directory if doesn't exist
     - Derive name from directory if not provided
     - Generate uid if not provided
     - Create `src/` directory
     - Write `project.json` with `{ uid, name }`
     - Call `create_default_template()` to create default nodes
   - `pub fn create_default_template(fs: &dyn LpFs) -> Result<()>`:
     - Create texture node at `/src/texture.texture/node.json`:
       - `{"$type":"Memory","size":[64,64],"format":"RGB8"}`
     - Create shader node at `/src/shader.shader/node.json`:
       - `{"$type":"Single","texture_id":"/src/texture.texture"}`
     - Create shader GLSL at `/src/shader.shader/main.glsl`:
       - Rainbow rotating color wheel shader (from template.rs)
     - Create output node at `/src/output.output/node.json`:
       - `{"$type":"gpio_strip","chip":"ws2812","gpio_pin":4,"count":128}`
     - Create fixture node at `/src/fixture.fixture/node.json`:
       - Circle-list mapping with 12 channels (from template.rs)
     - Use `LpFs` trait for all file operations (works with any filesystem)
   - `pub fn print_success_message(dir: &Path, name: &str)`:
     - Print success message with next steps
     - Show: `cd <name> && lp-cli dev ws://localhost:2812/`

4. Update `lp-app/apps/lp-cli/src/commands/create/mod.rs`:
   - Export `handle_create` function
   - Re-export submodules

5. Add tests:
   - Test project creation with defaults
   - Test project creation with custom name/uid
   - Test UID generation format
   - Test directory name derivation
   - Test default template creation (verify all nodes exist)
   - Test template works with memory filesystem
   - Test error cases (permission errors, etc.)

## Notes

- The template function should use `LpFs` trait so it works with any filesystem (including memory for tests)
- Adapt error handling from `ServerError` to `anyhow::Result` for CLI use
- Consider moving template function to `lp-shared` in the future so both `lp-server` and `lp-cli` can use it
- For now, duplicate the template logic in `lp-cli` (can refactor later)
- The template creates:
  - Texture node (64x64 RGB8 memory texture)
  - Shader node with rainbow rotating color wheel GLSL
  - Output node (WS2812, GPIO pin 4, 128 LEDs)
  - Fixture node (circle-list mapping with 12 channels)

## Success Criteria

- `lp-cli create <dir>` creates project structure
- Defaults work correctly (name from dir, auto-generated uid)
- `project.json` is created with correct format
- Success message shows next steps
- Tests pass
- Code compiles without warnings
