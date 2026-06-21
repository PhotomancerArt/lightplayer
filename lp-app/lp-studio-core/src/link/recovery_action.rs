use lpa_link::LinkProviderId;
use serde::{Deserialize, Serialize};

/// A structured next step the UI or a future agent can offer after an issue.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum RecoveryAction {
    Retry,
    ChooseSimulator,
    ChooseProvider { provider_id: LinkProviderId },
    UseCompatibleBrowser,
    Reconnect,
    FlashFirmware { firmware_id: Option<String> },
    ResetDevice,
    Disconnect,
    OpenHelp { topic: String },
    Ignore,
}
