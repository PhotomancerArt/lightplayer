use serde::{Deserialize, Serialize};

/// Low-level operation a link endpoint may be able to perform.
///
/// These are below Studio product capabilities and below the `lp-server`
/// protocol. For example, project upload is a server filesystem operation, while
/// raw filesystem image access is a link management operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LinkManagementOperation {
    /// Reset or reboot the endpoint/runtime.
    Reset,
    /// Flash firmware onto the endpoint.
    FlashFirmware,
    /// Read the raw filesystem image below the running server.
    ReadRawFilesystem,
    /// Write the raw filesystem image below the running server.
    WriteRawFilesystem,
    /// Read low-level logs from the endpoint/link.
    ReadLogs,
    /// Read low-level diagnostics from the endpoint/link.
    ReadDiagnostics,
}

/// Management capability surface advertised by a link endpoint.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkManagement {
    pub can_reset: bool,
    pub can_flash: bool,
    pub can_read_fs: bool,
    pub can_write_fs: bool,
    pub can_read_logs: bool,
    pub can_read_diagnostics: bool,
}

impl LinkManagement {
    pub fn diagnostics_only() -> Self {
        Self {
            can_read_diagnostics: true,
            ..Self::default()
        }
    }

    pub fn supports(&self, operation: LinkManagementOperation) -> bool {
        match operation {
            LinkManagementOperation::Reset => self.can_reset,
            LinkManagementOperation::FlashFirmware => self.can_flash,
            LinkManagementOperation::ReadRawFilesystem => self.can_read_fs,
            LinkManagementOperation::WriteRawFilesystem => self.can_write_fs,
            LinkManagementOperation::ReadLogs => self.can_read_logs,
            LinkManagementOperation::ReadDiagnostics => self.can_read_diagnostics,
        }
    }

    pub fn esp32_serial_base() -> Self {
        Self {
            can_reset: true,
            can_read_logs: true,
            can_read_diagnostics: true,
            ..Self::default()
        }
    }

    pub fn with_flash(mut self) -> Self {
        self.can_flash = true;
        self
    }

    pub fn with_raw_filesystem(mut self) -> Self {
        self.can_read_fs = true;
        self.can_write_fs = true;
        self
    }
}
