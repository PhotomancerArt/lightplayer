use lpc_model::{
    FrameId, ModelType, SlotFieldShape, SlotMapKeyShape, SlotMeta, SlotShape, SlotShapeId,
    SlotVariantShape, current_state_version,
};

pub(crate) fn id(value: &str) -> SlotShapeId {
    SlotShapeId::parse(value).unwrap()
}

pub(crate) fn version() -> FrameId {
    current_state_version()
}

pub(crate) fn mapping_shape() -> SlotShape {
    SlotShape::Enum {
        meta: SlotMeta::empty(),
        variants: vec![
            variant(
                "circle",
                record(vec![
                    field("center", value(ModelType::Vec2)),
                    field("radius", value(ModelType::F32)),
                ]),
            ),
            variant(
                "square",
                record(vec![
                    field("origin", value(ModelType::Vec2)),
                    field("size", value(ModelType::Vec2)),
                ]),
            ),
        ],
    }
}

pub(crate) fn record(fields: Vec<SlotFieldShape>) -> SlotShape {
    SlotShape::Record {
        meta: SlotMeta::empty(),
        fields,
    }
}

pub(crate) fn map(key: SlotMapKeyShape, value: SlotShape) -> SlotShape {
    SlotShape::Map {
        meta: SlotMeta::empty(),
        key,
        value: Box::new(value),
    }
}

pub(crate) fn option(some: SlotShape) -> SlotShape {
    SlotShape::Option {
        meta: SlotMeta::empty(),
        some: Box::new(some),
    }
}

pub(crate) fn reference(id: SlotShapeId) -> SlotShape {
    SlotShape::reference(id)
}

pub(crate) fn field(name: &str, shape: SlotShape) -> SlotFieldShape {
    SlotFieldShape::new(name, shape).unwrap()
}

pub(crate) fn variant(name: &str, shape: SlotShape) -> SlotVariantShape {
    SlotVariantShape::new(name, shape).unwrap()
}

pub(crate) fn value(ty: ModelType) -> SlotShape {
    SlotShape::value(ty)
}
