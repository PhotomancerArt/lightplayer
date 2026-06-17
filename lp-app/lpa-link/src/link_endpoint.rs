use serde::{Deserialize, Serialize};

use crate::{LinkEndpointId, LinkEndpointStatus, LinkManagement, LinkProviderId};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkEndpoint {
    pub id: LinkEndpointId,
    pub provider_id: LinkProviderId,
    pub label: String,
    pub status: LinkEndpointStatus,
    pub management: LinkManagement,
}

impl LinkEndpoint {
    pub fn new(
        id: impl Into<LinkEndpointId>,
        provider_id: impl Into<LinkProviderId>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            provider_id: provider_id.into(),
            label: label.into(),
            status: LinkEndpointStatus::Available,
            management: LinkManagement::default(),
        }
    }

    pub fn with_status(mut self, status: LinkEndpointStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_management(mut self, management: LinkManagement) -> Self {
        self.management = management;
        self
    }
}
