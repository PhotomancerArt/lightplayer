use crate::{ConnectedDeviceSummary, ProgressState, ProviderChoice, UxIssue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkState {
    SelectingProvider { providers: Vec<ProviderChoice> },
    StartingSimulator { progress: ProgressState },
    Connected { device: ConnectedDeviceSummary },
    Failed { issue: UxIssue },
}
