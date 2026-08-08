//! Request for materializing a visual product into a complete texture.

use lps_shared::TextureStorageFormat;

use super::{ConsumerPolicy, VisualSpace};

/// Texture render request issued by a consumer that needs a full materialized frame.
///
/// `space` and `policy` are the space-tagged half (plan D18). Their
/// defaults — `TwoD`, no policy — are exactly what every consumer sent
/// before spaces existed, so a caller that ignores them is asking for
/// today's behavior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderTextureRequest {
    pub width: u32,
    pub height: u32,
    pub format: TextureStorageFormat,
    pub time_seconds: f32,
    /// Space the consumer is asking in. A `OneD` request wants a `(N, 1)`
    /// strip target; the producer projects if it does not live there.
    pub space: VisualSpace,
    /// The consumer's projection policy, honored by the producer when the
    /// spaces disagree.
    pub policy: ConsumerPolicy,
}
