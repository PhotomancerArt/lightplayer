use alloc::string::String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareEndpointStatus {
    Available,
    InUse { claimant: String },
    Unavailable { reason: String },
    Reserved { reason: String },
}

impl HardwareEndpointStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Available => None,
            Self::InUse { claimant } => Some(claimant),
            Self::Unavailable { reason } | Self::Reserved { reason } => Some(reason),
        }
    }
}
