//! The push conversation, in one runtime-neutral place.
//!
//! Studio has had these three steps since the sim shipped — they are what
//! `StudioServerClient::open_library_project` does — but they lived above a
//! `StudioServerClient`, which owns a session, a log sink and a pull loop.
//! A device push has none of those: it runs inside a coarse effect that
//! borrowed one serial port for the duration, on whatever `ClientIo` that
//! platform has (Web Serial line framing in the browser, the fake device's
//! byte stream on the host). So the conversation lives here, over the plain
//! [`LpClient`], and both callers run the SAME one.
//!
//! ```text
//! 1. ListLoadedProjects  → which storage dir does this board run from?
//! 2. StopAll → clear → chunked writes → LoadProject   (project_deploy's
//!    order, file_sync_ops' chunking — unchanged, on purpose)
//! 3. HashPackage         → does the board hold exactly the library's bytes?
//! ```
//!
//! Step 1 is why the push is not simply "write to `demo`": a board flashed
//! by the CLI, or by an older Studio, runs from a dir of its own, and
//! writing beside it would leave two projects on a device that loads one.
//! Replacing the dir it ALREADY runs from is what makes a push idempotent.
//!
//! Step 3 is not optional. A serial wire drops bytes; a truncated write that
//! loaded anyway would leave a board running something no library has, and
//! the next sync verdict would be computed against a lie.

use crate::client::LpClient;
use crate::client_error::{ClientError, ClientResult};
use crate::client_io::ClientIo;

/// What a finished push did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushReport {
    /// The project storage dir under `/projects/` the bytes went to.
    pub storage_id: String,
    /// The package hash the device reported afterwards — equal to the
    /// expected one, or this would have been an error.
    pub hash: String,
}

/// Progress callback: a label, and a percent when there is an honest one.
pub type PushProgress<'a> = &'a mut dyn FnMut(String, Option<u8>);

/// Run the push conversation against a device that is already listening.
///
/// `fallback_storage_id` is used only when the board reports nothing loaded
/// — a freshly flashed board, which has no dir to replace.
pub async fn push_project<Io: ClientIo>(
    client: &mut LpClient<Io>,
    files: &[(String, Vec<u8>)],
    expected_hash: &str,
    fallback_storage_id: &str,
    progress: PushProgress<'_>,
) -> ClientResult<PushReport> {
    progress("Asking the board what it is running".to_string(), Some(5));
    let loaded = client.project_list_loaded().await?;
    let storage_id = loaded
        .value
        .first()
        .and_then(|project| storage_id_of(project.path.as_str()))
        .unwrap_or_else(|| fallback_storage_id.to_string());

    progress(format!("Sending the project to {storage_id}"), Some(20));
    let deploy = client.replace_and_load_project(&storage_id, files).await?;
    let _ = deploy.value;

    progress("Checking what the board received".to_string(), Some(85));
    let hash = client.hash_package(&storage_id).await?.value;
    if hash != expected_hash {
        // The bytes on the board are not the bytes in the library. Saying so
        // beats a green card over a project nobody has.
        return Err(ClientError::Protocol(format!(
            "the board ended up with different bytes than the library sent \
             (device {hash}, library {expected_hash}) — the project was not \
             fully written"
        )));
    }
    progress("Done".to_string(), Some(100));
    Ok(PushReport { storage_id, hash })
}

/// The storage dir name inside a reported project path (`/projects/demo` →
/// `demo`). `None` for a path that is not under `/projects/`, which is not a
/// dir a push may replace.
fn storage_id_of(path: &str) -> Option<String> {
    let rest = path
        .trim_start_matches('/')
        .strip_prefix("projects/")?
        .trim_matches('/');
    let id = rest.split('/').next()?;
    (!id.is_empty()).then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reported_path_names_the_storage_dir_a_push_replaces() {
        assert_eq!(storage_id_of("/projects/demo").as_deref(), Some("demo"));
        assert_eq!(storage_id_of("projects/demo/").as_deref(), Some("demo"));
        assert_eq!(
            storage_id_of("/projects/2026-08-30-porch/sub").as_deref(),
            Some("2026-08-30-porch")
        );
        // Not a project dir: a push must not replace it.
        assert_eq!(storage_id_of("/somewhere/else"), None);
        assert_eq!(storage_id_of("/projects/"), None);
    }
}
