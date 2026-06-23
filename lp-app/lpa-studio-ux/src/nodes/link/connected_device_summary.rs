use lpa_link::LinkProviderKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectedDeviceSummary {
    pub provider_id: LinkProviderKind,
    pub endpoint_id: String,
    pub session_id: String,
    pub label: String,
}

impl ConnectedDeviceSummary {
    pub fn new(
        provider_id: LinkProviderKind,
        endpoint_id: impl Into<String>,
        session_id: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            provider_id,
            endpoint_id: endpoint_id.into(),
            session_id: session_id.into(),
            label: label.into(),
        }
    }
}
