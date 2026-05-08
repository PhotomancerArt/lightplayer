//! Output sink leaf: fixtures push channel samples into an engine [`crate::runtime_buffer::RuntimeBuffer`];
//! [`crate::project_runtime::CoreProjectRuntime::tick`] flushes dirty sinks via [`crate::project_runtime::RuntimeServices`].

use alloc::boxed::Box;

use alloc::vec::Vec;

use lpc_model::{Revision, SlotPath, WithRevision};

use crate::node::{
    DestroyCtx, MemPressureCtx, NodeRuntime, NodeError, NodeResourceInitContext, PressureLevel,
    TickContext,
};
use crate::prop::ProducedSlotAccess;
use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferId};
use crate::runtime_product::RuntimeProduct;

#[derive(Default)]
struct EmptyProps;

impl ProducedSlotAccess for EmptyProps {
    fn get(&self, _path: &SlotPath) -> Option<(RuntimeProduct, Revision)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: Revision,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, Revision)> + 'a> {
        Box::new(core::iter::empty())
    }

    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, Revision)> + 'a> {
        Box::new(core::iter::empty())
    }
}

/// Pushed sink node (not a demand root): flushing runs after engine tick from project runtime services.
pub struct OutputNode {
    channel_buffer_id: Option<RuntimeBufferId>,
    props: EmptyProps,
}

impl OutputNode {
    #[must_use]
    pub fn new() -> Self {
        Self {
            channel_buffer_id: None,
            props: EmptyProps,
        }
    }

    pub fn channel_buffer_id(&self) -> Option<RuntimeBufferId> {
        self.channel_buffer_id
    }
}

impl NodeRuntime for OutputNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.channel_buffer_id.is_some() {
            return Ok(());
        }
        let id = ctx.insert_runtime_buffer(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::output_channels_u16(0, Vec::new()),
        ));
        self.channel_buffer_id = Some(id);
        Ok(())
    }

    fn runtime_output_sink_buffer_id(&self) -> Option<RuntimeBufferId> {
        self.channel_buffer_id
    }
    fn tick(&mut self, _ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx<'_>,
    ) -> Result<(), NodeError> {
        Ok(())
    }

    fn produced(&self) -> &dyn ProducedSlotAccess {
        &self.props
    }
}
