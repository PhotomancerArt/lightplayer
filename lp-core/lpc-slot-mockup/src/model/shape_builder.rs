use lpc_model::{
    FrameId, ModelType, SlotMapKeyShape, SlotMeta, SlotShapeChild, SlotShapeField, SlotShapeId,
    SlotShapeNode, SlotShapeVariant, current_state_version,
};

pub(crate) fn id(value: &str) -> SlotShapeId {
    SlotShapeId::parse(value).unwrap()
}

pub(crate) fn version() -> FrameId {
    current_state_version()
}

pub(crate) fn mapping_shape_nodes(
    root: &str,
    value_prefix: &str,
) -> Vec<(SlotShapeId, SlotShapeNode)> {
    use SlotShapeChild::Owned;
    vec![
        (
            id(root),
            SlotShapeNode::Enum {
                meta: SlotMeta::empty(),
                variants: vec![
                    variant("circle", Owned(id(&format!("{root}.circle")))),
                    variant("square", Owned(id(&format!("{root}.square")))),
                ],
            },
        ),
        record(
            &format!("{root}.circle"),
            vec![
                field(
                    "center",
                    Owned(id(&format!("{value_prefix}.circle.center"))),
                ),
                field(
                    "radius",
                    Owned(id(&format!("{value_prefix}.circle.radius"))),
                ),
            ],
        ),
        value(&format!("{value_prefix}.circle.center"), ModelType::Vec2),
        value(&format!("{value_prefix}.circle.radius"), ModelType::F32),
        record(
            &format!("{root}.square"),
            vec![
                field(
                    "origin",
                    Owned(id(&format!("{value_prefix}.square.origin"))),
                ),
                field("size", Owned(id(&format!("{value_prefix}.square.size")))),
            ],
        ),
        value(&format!("{value_prefix}.square.origin"), ModelType::Vec2),
        value(&format!("{value_prefix}.square.size"), ModelType::Vec2),
    ]
}

pub(crate) fn record(id_text: &str, fields: Vec<SlotShapeField>) -> (SlotShapeId, SlotShapeNode) {
    (
        id(id_text),
        SlotShapeNode::Record {
            meta: SlotMeta::empty(),
            fields,
        },
    )
}

pub(crate) fn map(
    id_text: &str,
    key: SlotMapKeyShape,
    value: SlotShapeChild,
) -> (SlotShapeId, SlotShapeNode) {
    (
        id(id_text),
        SlotShapeNode::Map {
            meta: SlotMeta::empty(),
            key,
            value,
        },
    )
}

pub(crate) fn option(id_text: &str, some: SlotShapeChild) -> (SlotShapeId, SlotShapeNode) {
    (
        id(id_text),
        SlotShapeNode::Option {
            meta: SlotMeta::empty(),
            some,
        },
    )
}

pub(crate) fn field(name: &str, shape: SlotShapeChild) -> SlotShapeField {
    SlotShapeField::new(name, shape).unwrap()
}

pub(crate) fn variant(name: &str, shape: SlotShapeChild) -> SlotShapeVariant {
    SlotShapeVariant::new(name, shape).unwrap()
}

pub(crate) fn value(id_text: &str, ty: ModelType) -> (SlotShapeId, SlotShapeNode) {
    (id(id_text), SlotShapeNode::value(ty))
}
