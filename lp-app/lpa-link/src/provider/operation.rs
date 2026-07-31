use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Low-level operation a link endpoint/session may be able to perform.
///
/// These operations are below Studio product actions and below the `lp-server`
/// protocol. For example, project upload is a server filesystem operation, while
/// raw filesystem image access is a link operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LinkOperation {
    /// Reset or reboot the endpoint/runtime.
    Reset,
    /// Flash firmware onto the endpoint.
    FlashFirmware,
    /// Erase the endpoint flash so the device returns to a blank state.
    EraseDeviceFlash,
    /// Read the raw filesystem image below the running server.
    ReadRawFilesystem,
    /// Write the raw filesystem image below the running server.
    WriteRawFilesystem,
    /// Write the boot-control sector: an instruction to the device's next
    /// boot, delivered through flash because a device that cannot boot has no
    /// other channel. See `lp-bootctl`.
    WriteBootControl,
    /// Read low-level logs from the endpoint/link.
    ReadLogs,
    /// Read low-level diagnostics from the endpoint/link.
    ReadDiagnostics,
}

/// Set of low-level link operations advertised by an endpoint.
///
/// This is intentionally a set of `LinkOperation` values rather than one bool
/// per operation so Studio UX, UI shells, and future agents can inspect and
/// present the operation surface generically.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkCapabilities {
    operations: BTreeSet<LinkOperation>,
}

impl LinkCapabilities {
    pub fn new(operations: impl IntoIterator<Item = LinkOperation>) -> Self {
        Self {
            operations: operations.into_iter().collect(),
        }
    }

    pub fn operations(&self) -> impl Iterator<Item = LinkOperation> + '_ {
        self.operations.iter().copied()
    }

    pub fn diagnostics_only() -> Self {
        Self::default().with(LinkOperation::ReadDiagnostics)
    }

    pub fn supports(&self, operation: LinkOperation) -> bool {
        self.operations.contains(&operation)
    }

    pub fn with(mut self, operation: LinkOperation) -> Self {
        self.operations.insert(operation);
        self
    }

    pub fn esp32_serial_base() -> Self {
        Self::default()
            .with(LinkOperation::Reset)
            .with(LinkOperation::ReadLogs)
            .with(LinkOperation::ReadDiagnostics)
    }

    pub fn with_flash(mut self) -> Self {
        self.operations.insert(LinkOperation::FlashFirmware);
        self
    }

    pub fn with_device_erase(mut self) -> Self {
        self.operations.insert(LinkOperation::EraseDeviceFlash);
        self
    }

    /// Advertise the raw-filesystem READ only.
    ///
    /// Split out of [`Self::with_raw_filesystem`] (M6): backup ships before
    /// restore, and advertising the write half early would let the UI offer
    /// an operation every provider answers with `unsupported`. The paired
    /// builder stays for M7, which completes the other half.
    pub fn with_raw_filesystem_read(mut self) -> Self {
        self.operations.insert(LinkOperation::ReadRawFilesystem);
        self
    }

    pub fn with_raw_filesystem(mut self) -> Self {
        self.operations.insert(LinkOperation::ReadRawFilesystem);
        self.operations.insert(LinkOperation::WriteRawFilesystem);
        self
    }

    pub fn with_boot_control(mut self) -> Self {
        self.operations.insert(LinkOperation::WriteBootControl);
        self
    }
}
