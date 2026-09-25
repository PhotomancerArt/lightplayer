//! The three coarse effects that are `lpa-client` conversations over a
//! borrowed wire — write the board manifest, push a project, remove a
//! project — run the same way on every transport whose wire the studio
//! reaches through a `ClientIo`: the tab emulator, and a Bluetooth link.
//!
//! One body, so the two cannot drift: nothing about the ready-wait, the
//! chunking, the stop/write/load order or the hash check is special-cased
//! per transport, which is what makes a green push over Bluetooth mean what
//! it means over a cable. (The serial provider runs the same `lpa-client`
//! functions below its own seam.)

use lpc_model::AsLpPath;

use super::device_transport::{DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress};

/// Where the board runtime manifest lives on a device — the path the
/// firmware's loader reads at boot (board-selection D4; effective next
/// restart).
const DEVICE_HARDWARE_MANIFEST_PATH: &str = "/hardware.json";

/// Whether `call` is one of the conversations [`run_wire_conversation`]
/// runs. A transport builds its io only for these.
pub fn is_wire_conversation(call: &DeviceEffectCall) -> bool {
    matches!(
        call,
        DeviceEffectCall::WriteHardwareManifest { .. }
            | DeviceEffectCall::PushProject { .. }
            | DeviceEffectCall::RemoveProject { .. }
    )
}

/// Run one conversation effect over `io`, which the caller borrowed with the
/// link's pump paused. Anything that is not a conversation is refused by
/// name — flashing and erasing are the transport's own business.
pub async fn run_wire_conversation(
    io: Box<dyn lpa_client::ClientIo>,
    call: DeviceEffectCall,
    progress: DeviceEffectProgress,
) -> Result<DeviceEffectFacts, String> {
    let mut client = lpa_client::LpClient::new(io).on_borrowed_wire();
    let mut report = |label: String, percent: Option<u8>| progress(label, percent);
    match call {
        // The REAL write, not a sim's restart: the board has a filesystem
        // and a loader that reads `/hardware.json` at boot. The wait and
        // the chunking are the serial arm's, for the serial arm's reasons —
        // a board formatting its littlefs does not answer writes, and one
        // big frame OOMs a decode.
        DeviceEffectCall::WriteHardwareManifest { manifest_json } => {
            lpa_client::wait_until_ready(&mut client, lpa_client::READY_ATTEMPTS, &mut report)
                .await
                .map_err(|error| format!("the board never became ready to write to: {error}"))?;
            lpa_client::write_file_in_chunks(
                &mut client,
                DEVICE_HARDWARE_MANIFEST_PATH.as_path(),
                manifest_json.as_bytes(),
                lpa_client::MANIFEST_CHUNK_BYTES,
                &mut report,
            )
            .await
            .map_err(|error| format!("device file write failed: {error}"))?;
            Ok(DeviceEffectFacts {
                summary: "board manifest written".to_string(),
                ..Default::default()
            })
        }
        DeviceEffectCall::PushProject {
            files,
            expected_hash,
            fallback_storage_id,
        } => {
            let pushed = lpa_client::push_project(
                &mut client,
                &files,
                &expected_hash,
                &fallback_storage_id,
                &mut report,
            )
            .await
            .map_err(|error| error.to_string())?;
            Ok(DeviceEffectFacts {
                summary: format!("project sent to {}", pushed.storage_id),
                ..Default::default()
            })
        }
        DeviceEffectCall::RemoveProject {
            fallback_storage_id,
        } => {
            let removed =
                lpa_client::remove_project(&mut client, &fallback_storage_id, &mut report)
                    .await
                    .map_err(|error| error.to_string())?;
            Ok(DeviceEffectFacts {
                summary: match removed.was_loaded {
                    true => format!("removed {}", removed.storage_id),
                    // Under-claim: the board had already stopped reporting
                    // the project, so "removed" would be a claim about
                    // something never seen.
                    false => format!(
                        "the board reported nothing loaded; cleared {}",
                        removed.storage_id
                    ),
                },
                ..Default::default()
            })
        }
        other => Err(format!("{other:?} is not a wire conversation")),
    }
}
