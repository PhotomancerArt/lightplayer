use anyhow::{Context, bail};
use lpc_model::ManifestCore;
use lpc_model::manifest::find_manifest_core;

use super::args::ShowArgs;

pub fn handle_show(args: ShowArgs) -> anyhow::Result<()> {
    let bytes = std::fs::read(&args.artifact)
        .with_context(|| format!("reading {}", args.artifact.display()))?;
    let Some(payload) = find_manifest_core(&bytes) else {
        bail!(
            "no firmware manifest core found in {} — either the artifact predates \
             the embedded manifest (M2) or it is not a LightPlayer firmware build",
            args.artifact.display()
        );
    };
    let json = core::str::from_utf8(payload).context("manifest payload is not UTF-8")?;
    if args.raw {
        println!("{json}");
        return Ok(());
    }
    let core: ManifestCore = serde_json::from_str(json)
        .context("manifest payload does not parse as a known ManifestCore shape")?;
    println!("{}", serde_json::to_string_pretty(&core)?);
    Ok(())
}
