use anyhow::Result;

use super::args::CreateArgs;
use super::project;

pub fn handle_create(args: CreateArgs) -> Result<()> {
    // Derive name from directory if not provided
    let name = if let Some(ref name) = args.name {
        name.clone()
    } else {
        project::derive_project_name(&args.dir)
    };

    // Create project structure
    project::create_project_structure(&args.dir, args.name.as_deref(), args.uid.as_deref())?;

    // Print success message
    project::print_success_message(&args.dir, &name);

    Ok(())
}
