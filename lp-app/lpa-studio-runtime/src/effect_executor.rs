use lpa_studio_core::{StudioEffect, StudioEvent};

use crate::StudioRuntimeError;

#[allow(
    async_fn_in_trait,
    reason = "Studio runtime executors are not object-safe yet"
)]
pub trait EffectExecutor {
    async fn execute_effect(
        &mut self,
        effect: StudioEffect,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError>;
}
