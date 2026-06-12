use anyhow::{Context, Result, anyhow, bail};
use lpc_hardware::HardwareManifestFile;
use std::fs;
use std::path::{Path, PathBuf};

pub struct BoardManifestStore {
    boards_dir: PathBuf,
}

impl BoardManifestStore {
    pub fn discover(repo: Option<PathBuf>, boards_dir: Option<PathBuf>) -> Result<Self> {
        let repo_root = find_repo_root(repo)?;
        let boards_dir = match boards_dir {
            Some(path) => path,
            None => repo_root.join("lp-core/lpc-hardware/boards"),
        };
        Ok(Self { boards_dir })
    }

    pub fn boards_dir(&self) -> &Path {
        &self.boards_dir
    }

    pub fn list(&self) -> Result<Vec<BoardManifestSummary>> {
        if !self.boards_dir.exists() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for vendor_entry in fs::read_dir(&self.boards_dir)
            .with_context(|| format!("failed to read {}", self.boards_dir.display()))?
        {
            let vendor_entry = vendor_entry?;
            if !vendor_entry.file_type()?.is_dir() {
                continue;
            }
            for file_entry in fs::read_dir(vendor_entry.path())? {
                let file_entry = file_entry?;
                let path = file_entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                    continue;
                }
                let manifest = self.load_path(&path)?;
                out.push(BoardManifestSummary {
                    id: manifest.id,
                    target: manifest.target.to_string(),
                    vendor: manifest.vendor,
                    product: manifest.product,
                    path,
                });
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    pub fn load(&self, id: &str) -> Result<HardwareManifestFile> {
        let path = self.path_for_id(id)?;
        self.load_path(&path)
    }

    pub fn save(&self, manifest: &HardwareManifestFile, overwrite: bool) -> Result<PathBuf> {
        manifest.validate().map_err(|error| anyhow!("{error}"))?;
        let path = self.path_for_id(&manifest.id)?;
        if path.exists() && !overwrite {
            bail!(
                "manifest {} already exists at {}",
                manifest.id,
                path.display()
            );
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = manifest.write_toml().map_err(|error| anyhow!("{error}"))?;
        fs::write(&path, text)?;
        Ok(path)
    }

    pub fn delete(&self, id: &str) -> Result<PathBuf> {
        let path = self.path_for_id(id)?;
        fs::remove_file(&path)
            .with_context(|| format!("failed to delete manifest {}", path.display()))?;
        Ok(path)
    }

    pub fn validate_all(&self) -> Result<Vec<(String, Result<()>)>> {
        let summaries = self.list()?;
        Ok(summaries
            .into_iter()
            .map(|summary| {
                let id = summary.id.clone();
                let result = self
                    .load(&summary.id)
                    .and_then(|manifest| manifest.validate().map_err(|error| anyhow!("{error}")));
                (id, result)
            })
            .collect())
    }

    fn load_path(&self, path: &Path) -> Result<HardwareManifestFile> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest {}", path.display()))?;
        let manifest =
            HardwareManifestFile::read_toml(&text).map_err(|error| anyhow!("{error}"))?;
        manifest.validate().map_err(|error| anyhow!("{error}"))?;
        Ok(manifest)
    }

    fn path_for_id(&self, id: &str) -> Result<PathBuf> {
        validate_manifest_id(id)?;
        let (vendor, name) = id
            .split_once('/')
            .ok_or_else(|| anyhow!("manifest id must look like vendor/name"))?;
        Ok(self.boards_dir.join(vendor).join(format!("{name}.toml")))
    }
}

#[derive(Debug, Clone)]
pub struct BoardManifestSummary {
    pub id: String,
    pub target: String,
    pub vendor: String,
    pub product: String,
    pub path: PathBuf,
}

pub fn validate_manifest_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.starts_with('/')
        || id.contains("..")
        || id.split('/').count() != 2
        || id.split('/').any(str::is_empty)
    {
        bail!("invalid manifest id: {id}");
    }
    for segment in id.split('/') {
        if !segment
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            bail!("invalid manifest id segment: {segment}");
        }
    }
    Ok(())
}

pub fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in value.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn find_repo_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }

    let cwd = std::env::current_dir()?;
    for candidate in cwd.ancestors() {
        if candidate.join("Cargo.toml").exists() && candidate.join("lp-core/lpc-shared").exists() {
            return Ok(candidate.to_path_buf());
        }
    }
    bail!("could not find repository root from {}", cwd.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_manifest_ids() {
        assert!(validate_manifest_id("seeed/xiao-esp32-c6").is_ok());
        assert!(validate_manifest_id("../x").is_err());
        assert!(validate_manifest_id("seeed/XIAO").is_err());
        assert!(validate_manifest_id("seeed/x/y").is_err());
    }

    #[test]
    fn slugifies_product_names() {
        assert_eq!(slugify("XIAO ESP32-C6"), "xiao-esp32-c6");
    }
}
