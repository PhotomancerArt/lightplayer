//! Output sink leaf: fixtures push channel samples into an engine [`crate::runtime_buffer::RuntimeBuffer`];
//! [`crate::project_runtime::CoreProjectRuntime::tick`] flushes dirty sinks via [`crate::project_runtime::RuntimeServices`].

use alloc::boxed::Box;

use alloc::vec::Vec;

use lpc_model::prop::PropPath;
use lpc_model::{FrameId, Versioned};
use lps_shared::LpsValueF32;

use crate::node::{
    DestroyCtx, MemPressureCtx, Node, NodeError, NodeResourceInitContext, PressureLevel,
    TickContext,
};
use crate::prop::RuntimePropAccess;
use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferId};

#[derive(Default)]
struct EmptyProps;

impl RuntimePropAccess for EmptyProps {
    fn get(&self, _path: &PropPath) -> Option<(LpsValueF32, FrameId)> {
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        _since: FrameId,
    ) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
        Box::new(core::iter::empty())
    }

    fn snapshot<'a>(&'a self) -> Box<dyn Iterator<Item = (PropPath, LpsValueF32, FrameId)> + 'a> {
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

impl Node for OutputNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.channel_buffer_id.is_some() {
            return Ok(());
        }
        let id = ctx.insert_runtime_buffer(Versioned::new(
            FrameId::default(),
            RuntimeBuffer::raw(Vec::new()),
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

    fn props(&self) -> &dyn RuntimePropAccess {
        &self.props
    }
}
