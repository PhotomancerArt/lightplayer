# Phase 6: Update Dev Command to Stop All Projects

## Scope of phase

Update the dev command handler to call `stop_all_projects()` before pushing a project to the server. This ensures a clean state before loading the new project.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Dev Command Handler

**File**: `lp-cli/src/commands/dev/handler.rs`

Update `handle_dev_async()` to stop all projects before pushing:

```rust
async fn handle_dev_async(
    args: DevArgs,
    project_uid: String,
    host_spec: HostSpecifier,
) -> Result<()> {
    // Format host specifier for error messages before it's moved
    let host_spec_str = format!("{host_spec:?}");

    // Connect to server
    let transport = client_connect(host_spec).context("Failed to connect to server")?;

    // Wrap transport in Arc<Mutex> for sharing
    let shared_transport = Arc::new(tokio::sync::Mutex::new(transport));

    // Create LpClient with shared transport
    let client = Arc::new(LpClient::new_shared(Arc::clone(&shared_transport)));

    // Create local filesystem
    let local_fs: Arc<dyn LpFs> = Arc::new(LpFsStd::new(args.dir.clone()));

    // Stop all currently loaded projects before pushing
    // This ensures a clean state
    if let Err(e) = client.stop_all_projects().await {
        // Log warning but continue - server might not have any projects loaded
        eprintln!("Warning: Failed to stop all projects: {e}");
        eprintln!("Continuing with project push...");
    }

    // Push project to server
    // This ensures the project exists on the server before we try to load it
    push_project_async(&client, &*local_fs, &project_uid)
        .await
        .with_context(|| format!("Failed to push project to server (host: {host_spec_str})"))?;

    // Load project on server
    let project_path = format!("projects/{project_uid}");
    let project_handle = client
        .project_load(&project_path)
        .await
        .context("Failed to load project on server")?;

    // ... rest of the function remains the same ...
}
```

### 2. Add Import

Make sure `LpClient` has the `stop_all_projects` method available (should be from Phase 1).

## Validate

Run the following commands to validate the phase:

```bash
cd lp-cli
cargo check
cargo test
```

Fix any warnings or errors before proceeding.
