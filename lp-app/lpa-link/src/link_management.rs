use serde::{Deserialize, Serialize};

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
}
