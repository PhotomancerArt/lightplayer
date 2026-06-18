use lpa_link::{LinkEndpoint, LinkProviderId};
use serde::{Deserialize, Serialize};

use crate::BROWSER_WORKER_PROVIDER_ID;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LinkSelection {
    pub selected_provider_id: LinkProviderId,
    pub endpoints: Vec<LinkEndpoint>,
}

impl LinkSelection {
    pub fn new(selected_provider_id: LinkProviderId) -> Self {
        Self {
            selected_provider_id,
            endpoints: Vec::new(),
        }
    }
}

impl Default for LinkSelection {
    fn default() -> Self {
        Self::new(LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID))
    }
}
