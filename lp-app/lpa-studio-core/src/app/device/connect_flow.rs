//! The connect-flow view state: where the picker/open sequence stands.
//!
//! [`DeviceController`] drives this alongside the runtime pool: the flow
//! narrates the catalog → discovery → endpoint → connect sequence for the
//! views (gallery issue chip, card connect narration), while the
//! pool's [`RuntimeSession`] holds what actually got connected.
//! `Connected` is entered exactly when a connect flow hands a live
//! session payload to the pool.
//!
//! There is deliberately no `Managing` variant: management runs inside
//! the hardware `DeviceSession` and never leaves the flow's `Connected`.
//!
//! [`DeviceController`]: super::DeviceController
//! [`RuntimeSession`]: crate::RuntimeSession

use crate::{ConnectedDeviceSummary, EndpointChoice, ProgressState, ProviderChoice, UiIssue};
use lpa_link::LinkProviderKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectFlowState {
    SelectingProvider {
        providers: Vec<ProviderChoice>,
        issue: Option<UiIssue>,
    },
    DiscoveringEndpoints {
        provider_id: LinkProviderKind,
        progress: ProgressState,
    },
    SelectingEndpoint {
        provider_id: LinkProviderKind,
        endpoints: Vec<EndpointChoice>,
    },
    Connecting {
        endpoint: EndpointChoice,
        progress: ProgressState,
    },
    /// The connect retry ladder's middle rung (M6, the D31 replacement):
    /// the first attempt failed, the automatic reset ran (opening a
    /// session resets the board — DTR/RTS), and a second attempt is in
    /// flight. The card narrates "Resetting…" through
    /// [`ConnectEvidence::Connecting`](crate::ConnectEvidence).
    Retrying {
        endpoint: EndpointChoice,
        progress: ProgressState,
    },
    /// The port's open failed as held-by-another-holder (D32 soft
    /// failure): the card shows In-use-elsewhere and a quiet periodic
    /// retry runs on the tick cadence. Never a toast.
    PortHeld {
        endpoint: EndpointChoice,
    },
    /// The ladder exhausted without a live session: the card shows the
    /// honest Not-responding state; Troubleshoot is its affordance.
    /// Distinct from [`Self::Failed`] — no gallery issue chip shouts.
    Unresponsive {
        endpoint: EndpointChoice,
    },
    Connected {
        device: ConnectedDeviceSummary,
    },
    Failed {
        issue: UiIssue,
    },
}

impl ConnectFlowState {
    /// Compact, stable label for event-log records (part of the JSONL trace
    /// contract — extend, do not rename).
    pub fn label(&self) -> &'static str {
        match self {
            Self::SelectingProvider { .. } => "selecting-provider",
            Self::DiscoveringEndpoints { .. } => "discovering-endpoints",
            Self::SelectingEndpoint { .. } => "selecting-endpoint",
            Self::Connecting { .. } => "connecting",
            Self::Retrying { .. } => "retrying",
            Self::PortHeld { .. } => "port-held",
            Self::Unresponsive { .. } => "unresponsive",
            Self::Connected { .. } => "connected",
            Self::Failed { .. } => "failed",
        }
    }
}
