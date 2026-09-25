//! The project sidecar: who can use THIS piece over Bluetooth.
//!
//! `<project>/.lp/access.json` ([`lpc_access::ProjectAccessFile`]) lives in
//! the library package and travels to a device with every push (M3 confirmed
//! push carries it). It never leaves the library otherwise: zip export,
//! share and publish skip it (M3), and a device will not read it back. The
//! library copy is the source; Studio reads and writes it here.

use lpc_access::{ProjectAccessFile, SALT_BYTES};
use lpc_model::AsLpPath;
use lpfs::LpFs;

use super::device_access_record::NewSecret;
use super::ui_access_view::UiAccessSecret;

/// The sidecar as it stands in a package (an absent file is an empty list).
pub fn read_project_access(fs: &dyn LpFs) -> Result<ProjectAccessFile, String> {
    let path = ProjectAccessFile::RELATIVE_PATH.as_path();
    match fs.read_file(path) {
        Ok(bytes) => ProjectAccessFile::from_json(&bytes).map_err(|error| error.to_string()),
        Err(_) if !fs.file_exists(path).unwrap_or(false) => Ok(ProjectAccessFile::new(Vec::new())),
        Err(error) => Err(error.to_string()),
    }
}

/// The list, as the project's settings show it.
pub fn project_access_secrets(file: &ProjectAccessFile) -> Vec<UiAccessSecret> {
    file.secrets
        .iter()
        .map(|entry| UiAccessSecret {
            label: entry.label.clone(),
            tier: entry.tier,
        })
        .collect()
}

/// Add (or replace, by label) one password, deriving its key now.
pub fn add_project_secret(
    fs: &dyn LpFs,
    secret: &NewSecret,
    salt: [u8; SALT_BYTES],
    iterations: u32,
) -> Result<(), String> {
    let current = read_project_access(fs)?;
    let store = super::device_access_record::apply_access_change(
        Some(&lpc_access::DeviceAccessFile {
            version: lpc_access::DeviceAccessFile::VERSION,
            secrets: current.secrets,
            ble_enabled: false,
            open: false,
        }),
        &super::DeviceAccessChange::Add(secret.clone()),
        salt,
        iterations,
    )?;
    write(fs, ProjectAccessFile::new(store.secrets))
}

/// Remove one password by label.
pub fn revoke_project_secret(fs: &dyn LpFs, label: &str) -> Result<(), String> {
    let mut current = read_project_access(fs)?;
    current.secrets.retain(|entry| entry.label != label);
    write(fs, current)
}

fn write(fs: &dyn LpFs, file: ProjectAccessFile) -> Result<(), String> {
    let json = file.to_json().map_err(|error| error.to_string())?;
    fs.write_file(ProjectAccessFile::RELATIVE_PATH.as_path(), json.as_bytes())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_access::Tier;
    use lpfs::LpFsMemory;

    #[test]
    fn the_list_is_added_to_by_label_and_revoked_from() {
        let fs = LpFsMemory::new();
        assert!(read_project_access(&fs).unwrap().secrets.is_empty());
        let camp = NewSecret {
            label: "camp".to_string(),
            tier: Tier::Play,
            password: "smores".to_string(),
        };
        add_project_secret(&fs, &camp, [4; 16], 2).unwrap();
        add_project_secret(
            &fs,
            &NewSecret {
                label: "crew".to_string(),
                tier: Tier::Edit,
                password: "x".to_string(),
            },
            [5; 16],
            2,
        )
        .unwrap();
        let file = read_project_access(&fs).unwrap();
        assert_eq!(
            project_access_secrets(&file),
            [
                UiAccessSecret {
                    label: "camp".to_string(),
                    tier: Tier::Play
                },
                UiAccessSecret {
                    label: "crew".to_string(),
                    tier: Tier::Edit
                }
            ]
        );
        revoke_project_secret(&fs, "camp").unwrap();
        assert_eq!(read_project_access(&fs).unwrap().secrets.len(), 1);
        // What was written is the sidecar a device verifies against.
        let bytes = fs
            .read_file("/.lp/access.json".as_path())
            .expect("the sidecar is written at its path");
        assert!(ProjectAccessFile::from_json(&bytes).is_ok());
    }
}
