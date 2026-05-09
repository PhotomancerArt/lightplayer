//! Core texture node: width/height/format metadata for shader output sizing.

use lpc_model::NodeId;
use lpc_model::SlotAccess;
#[cfg(test)]
use lpc_model::SlotPath;
use lpc_model::SlotShapeRegistry;
use lpc_model::SlotShapeRegistryError;
use lpc_model::StaticSlotShape;
use lpc_model::nodes::texture::TextureState;
use lpc_model::nodes::texture::{TextureDef, TextureFormat};

use crate::node::{DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, TickContext};

#[cfg(test)]
fn width_path() -> SlotPath {
    SlotPath::parse("width").expect("width path")
}

fn texture_format_tag(f: TextureFormat) -> u32 {
    match f {
        TextureFormat::Rgb8 => 0,
        TextureFormat::Rgba8 => 1,
        TextureFormat::R8 => 2,
        TextureFormat::Rgba16 => 3,
    }
}

/// MVP texture node: preserves [`TextureDef`] on the core engine tree.
pub struct TextureNode {
    node_id: NodeId,
    config: TextureDef,
    pixel_format: TextureFormat,
    state: TextureState,
}

impl TextureNode {
    pub fn new(node_id: NodeId, config: TextureDef) -> Self {
        let pixel_format = TextureFormat::Rgba16;
        let width = i32::try_from(config.width()).unwrap_or(i32::MAX);
        let height = i32::try_from(config.height()).unwrap_or(i32::MAX);
        Self {
            node_id,
            config,
            pixel_format,
            state: TextureState::new(width, height, texture_format_tag(pixel_format)),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn config(&self) -> &TextureDef {
        &self.config
    }

    pub fn pixel_format(&self) -> TextureFormat {
        self.pixel_format
    }
}

impl NodeRuntime for TextureNode {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        self.state.sync_with_revision(
            ctx.revision(),
            i32::try_from(self.config.width()).unwrap_or(i32::MAX),
            i32::try_from(self.config.height()).unwrap_or(i32::MAX),
            texture_format_tag(self.pixel_format),
        );
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

    fn runtime_state_slots(&self) -> &dyn SlotAccess {
        &self.state
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        TextureState::ensure_registered(registry).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::engine::resolve_with_engine_host;
    use crate::node::test_placeholder_spine;
    use crate::resolver::{QueryKey, ResolveLogLevel};
    use alloc::boxed::Box;
    use lpc_model::{Revision, TreePath};
    use lpc_wire::{WireChildKind, WireSlotIndex};
    use lps_shared::LpsValueF32;

    #[test]
    fn texture_metadata_props_resolve_on_engine() {
        let mut engine = Engine::new(TreePath::parse("/t.show").expect("path"));
        let frame = Revision::new(1);
        let root = engine.tree().root();
        let (spine, artifact) = test_placeholder_spine();
        let tid = engine
            .tree_mut()
            .add_child(
                root,
                lpc_model::NodeName::parse("tex").expect("name"),
                lpc_model::NodeName::parse("texture").expect("ty"),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine,
                artifact,
                frame,
            )
            .expect("add");
        let tex = TextureNode::new(tid, TextureDef::new(64, 48));
        engine
            .attach_runtime_node(tid, Box::new(tex), frame)
            .expect("attach");

        let w = QueryKey::ConsumedSlot {
            node: tid,
            slot: width_path(),
        };
        let pv = resolve_with_engine_host(&mut engine, w, ResolveLogLevel::Off)
            .expect("resolve")
            .0;
        assert!(matches!(
            pv.as_value().expect("value"),
            LpsValueF32::I32(64)
        ));
    }
}
