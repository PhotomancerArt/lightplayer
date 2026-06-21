use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DeviceCapability {
    /// The provider can request or report user/device access permission.
    RequestDeviceAccess,
    /// The endpoint can open a client connection to a running `lp-server`.
    Connect,
    /// The endpoint is an ESP32 reached through browser Web Serial.
    UseBrowserSerialEsp32,
    /// The endpoint is an ESP32 reached through host OS serial.
    UseHostSerialEsp32,
    /// The endpoint is a browser worker running `fw-browser`.
    UseBrowserWorker,
    /// The endpoint is an in-process host runtime running `fw-host`.
    UseHostProcess,
    /// The endpoint can reset or reboot the underlying device/runtime.
    ResetDevice,
    /// The endpoint can flash firmware onto the underlying device.
    FlashFirmware,
    /// The server connection can write project files.
    WriteProjectFiles,
    /// The link can read a raw filesystem image below the running server.
    ReadRawFilesystem,
    /// The link can write a raw filesystem image below the running server.
    WriteRawFilesystem,
    /// The server connection can report heartbeat/status messages.
    ReadHeartbeat,
    /// The server connection can list loaded or available projects.
    ListProjects,
    /// The server connection can load a project.
    LoadProject,
    /// The server connection can read project inventory.
    ReadProjectInventory,
    /// The link or server can surface logs.
    ReadLogs,
    /// The link or server can surface diagnostics.
    ReadDiagnostics,
}
