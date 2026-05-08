//! Core texture node: width/height/format metadata for shader output sizing.

use alloc::boxed::Box;
use alloc::vec;

use lpc_model::FrameId;
use lpc_model::NodeId;
use lpc_model::SlotPath;
use lpc_source::node::texture::{TextureDef, TextureFormat};
use lps_shared::LpsValueF32;

use crate::node::{DestroyCtx, MemPressureCtx, Node, NodeError, PressureLevel, TickContext};
use crate::prop::ProducedSlotAccess;
use crate::runtime_product::RuntimeProduct;

fn width_path() -> SlotPath {
    SlotPath::parse("width").expect("width path")
}

fn height_path() -> SlotPath {
    SlotPath::parse("height").expect("height path")
}

fn format_path() -> SlotPath {
    SlotPath::parse("format").expect("format path")
}

/// [`NodeId`] of the texture and conventional prop paths for width/height (used by shader nodes).
pub(crate) fn texture_dimension_query_targets(
    texture_node_id: NodeId,
) -> (NodeId, SlotPath, SlotPath) {
    (texture_node_id, width_path(), height_path())
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
    props: TextureProps,
}

#[derive(Clone)]
struct TextureProps {
    width_path: SlotPath,
    height_path: SlotPath,
    format_path: SlotPath,
    width: i32,
    height: i32,
    format_tag: u32,
    frame: FrameId,
}

impl TextureProps {
    fn sync(&mut self, config: &TextureDef, pixel_format: TextureFormat, frame: FrameId) {
        self.width = i32::try_from(config.width()).unwrap_or(i32::MAX);
        self.height = i32::try_from(config.height()).unwrap_or(i32::MAX);
        self.format_tag = texture_format_tag(pixel_format);
        self.frame = frame;
    }
}

impl ProducedSlotAccess for TextureProps {
    fn get(&self, path: &SlotPath) -> Option<(RuntimeProduct, FrameId)> {
        if path == &self.width_path {
            return Some((
                RuntimeProduct::Value(LpsValueF32::I32(self.width)),
                self.frame,
            ));
        }
        if path == &self.height_path {
            return Some((
                RuntimeProduct::Value(LpsValueF32::I32(self.height)),
                self.frame,
            ));
        }
        if path == &self.format_path {
            return Some((
                RuntimeProduct::Value(LpsValueF32::U32(self.format_tag)),
                self.frame,
            ));
        }
        None
    }

    fn iter_changed_since<'a>(
        &'a self,
        since: FrameId,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, FrameId)> + 'a> {
        if self.frame.as_i64() <= since.as_i64() {
            return Box::new(core::iter::empty());
        }
        Box::new(
            vec![
                (
                    self.width_path.clone(),
                    RuntimeProduct::Value(LpsValueF32::I32(self.width)),
                    self.frame,
                ),
                (
                    self.height_path.clone(),
                    RuntimeProduct::Value(LpsValueF32::I32(self.height)),
                    self.frame,
                ),
                (
                    self.format_path.clone(),
                    RuntimeProduct::Value(LpsValueF32::U32(self.format_tag)),
                    self.frame,
                ),
            ]
            .into_iter(),
        )
    }

    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (SlotPath, RuntimeProduct, FrameId)> + 'a> {
        Box::new(
            vec![
                (
                    self.width_path.clone(),
                    RuntimeProduct::Value(LpsValueF32::I32(self.width)),
                    self.frame,
                ),
                (
                    self.height_path.clone(),
                    RuntimeProduct::Value(LpsValueF32::I32(self.height)),
                    self.frame,
                ),
                (
                    self.format_path.clone(),
                    RuntimeProduct::Value(LpsValueF32::U32(self.format_tag)),
                    self.frame,
                ),
            ]
            .into_iter(),
        )
    }
}

impl TextureNode {
    pub fn new(node_id: NodeId, config: TextureDef) -> Self {
        let pixel_format = TextureFormat::Rgba16;
        let mut props = TextureProps {
            width_path: width_path(),
            height_path: height_path(),
            format_path: format_path(),
            width: 0,
            height: 0,
            format_tag: texture_format_tag(pixel_format),
            frame: FrameId::default(),
        };
        props.sync(&config, pixel_format, FrameId::default());
        Self {
            node_id,
            config,
            pixel_format,
            props,
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

impl Node for TextureNode {
    fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        self.props
            .sync(&self.config, self.pixel_format, ctx.frame_id());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::engine::resolve_with_engine_host;
    use crate::resolver::{QueryKey, ResolveLogLevel};
    use crate::tree::test_placeholder_spine;
    use lpc_model::TreePath;
    use lpc_wire::{WireChildKind, WireSlotIndex};

    #[test]
    fn texture_metadata_props_resolve_on_engine() {
        let mut engine = Engine::new(TreePath::parse("/t.show").expect("path"));
        let frame = FrameId::new(1);
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
