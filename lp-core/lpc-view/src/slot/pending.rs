use lpc_wire::{WireSlotMutationRequest, WireSlotMutationResponse};

/// Client-side pending mutation metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingSlotMutation {
    pub request: WireSlotMutationRequest,
}

impl PendingSlotMutation {
    pub fn new(request: WireSlotMutationRequest) -> Self {
        Self { request }
    }

    pub fn matches_response(&self, response: &WireSlotMutationResponse) -> bool {
        self.request.id == response.id
    }
}
