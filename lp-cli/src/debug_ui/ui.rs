//! Minimal debug UI shell between legacy demolition and generic slot UI rebuild.

use crate::client::LpClient;
use eframe::egui;
use lpc_model::{
    LpType, LpValue, NodeId, SlotData, SlotMapKey, SlotShape, SlotShapeId, SlotShapeRegistry,
};
use lpc_view::apply_project_read_response;
use lpc_view::project::ProjectView;
use lpc_wire::WireProjectHandle as ProjectHandle;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type ProjectReadResult = Result<lpc_wire::ProjectReadResponse, String>;

/// Debug UI application state.
pub struct DebugUiState {
    project_view: Arc<Mutex<lpc_view::project::ProjectView>>,
    project_handle: ProjectHandle,
    async_client: LpClient,
    runtime_handle: tokio::runtime::Handle,
    response_tx: std::sync::mpsc::Sender<ProjectReadResult>,
    response_rx: std::sync::mpsc::Receiver<ProjectReadResult>,
    last_poll: Instant,
    poll_in_flight: bool,
    last_error: Option<String>,
    selected_node: Option<NodeId>,
}

impl DebugUiState {
    /// Create new debug UI state.
    pub fn new(
        project_view: Arc<Mutex<lpc_view::project::ProjectView>>,
        project_handle: ProjectHandle,
        async_client: LpClient,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let (response_tx, response_rx) = std::sync::mpsc::channel();
        Self {
            project_view,
            project_handle,
            async_client,
            runtime_handle,
            response_tx,
            response_rx,
            last_poll: Instant::now() - Duration::from_secs(1),
            poll_in_flight: false,
            last_error: None,
            selected_node: None,
        }
    }

    fn drain_project_reads(&mut self) {
        while let Ok(result) = self.response_rx.try_recv() {
            self.poll_in_flight = false;
            match result {
                Ok(response) => {
                    if let Ok(mut view) = self.project_view.lock() {
                        if let Err(error) = apply_project_read_response(&mut view, response) {
                            self.last_error = Some(error.to_string());
                        } else {
                            self.last_error = None;
                        }
                    }
                }
                Err(error) => {
                    self.last_error = Some(error);
                }
            }
        }
    }

    fn poll_project_if_due(&mut self, ctx: &egui::Context) {
        if self.poll_in_flight || self.last_poll.elapsed() < Duration::from_millis(500) {
            return;
        }

        self.last_poll = Instant::now();
        self.poll_in_flight = true;
        let client = self.async_client.clone();
        let handle = self.project_handle;
        let tx = self.response_tx.clone();
        let repaint = ctx.clone();
        self.runtime_handle.spawn(async move {
            let result = client
                .project_read_default_debug(handle)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
    }
}

impl eframe::App for DebugUiState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_project_reads();
        self.poll_project_if_due(ctx);

        egui::TopBottomPanel::top("lp_status").show(ctx, |ui| {
            self.render_status(ui);
        });

        egui::SidePanel::left("lp_node_tree")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                self.render_node_tree(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_selected_node(ui);
        });

        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

impl DebugUiState {
    fn render_status(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("LightPlayer Dev UI");
            ui.separator();
            ui.label(format!("Project {}", self.project_handle.id()));
            ui.separator();
            ui.label(if self.poll_in_flight {
                "sync polling"
            } else {
                "sync idle"
            });

            if let Ok(view) = self.project_view.lock() {
                ui.separator();
                ui.label(format!("rev {}", view.revision.0));
                ui.label(format!("nodes {}", view.tree.nodes.len()));
                ui.label(format!("slots {}", view.slots.roots.len()));
                ui.label(format!("resources {}", view.resource_cache.summary_count()));
            }
        });

        if let Some(error) = &self.last_error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }

    fn render_node_tree(&mut self, ui: &mut egui::Ui) {
        ui.heading("Nodes");
        ui.separator();

        let Ok(view) = self.project_view.lock() else {
            ui.label("Project view locked");
            return;
        };

        if view.tree.nodes.is_empty() {
            ui.label("Waiting for project sync...");
            return;
        }

        if self
            .selected_node
            .is_none_or(|id| !view.tree.nodes.contains_key(&id))
        {
            self.selected_node = view.tree.nodes.keys().next().copied();
        }

        let roots: Vec<NodeId> = view
            .tree
            .nodes
            .iter()
            .filter_map(|(id, entry)| entry.parent.is_none().then_some(*id))
            .collect();

        for root in roots {
            render_node_tree_entry(ui, &view, root, 0, &mut self.selected_node);
        }
    }

    fn render_selected_node(&mut self, ui: &mut egui::Ui) {
        let Ok(view) = self.project_view.lock() else {
            ui.label("Project view locked");
            return;
        };

        let Some(selected) = self.selected_node else {
            ui.heading("No Node Selected");
            ui.label("Waiting for project sync...");
            return;
        };

        let Some(entry) = view.tree.nodes.get(&selected) else {
            ui.heading("No Node Selected");
            return;
        };

        ui.heading(entry.path.to_string());
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("id {}", selected.0));
            ui.separator();
            ui.label(format!("status {:?}", entry.status));
            ui.separator();
            ui.label(format!("state {:?}", entry.state));
            ui.separator();
            ui.label(format!("changed {}", entry.change_frame.0));
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            render_node_slots(ui, &view, selected);
            ui.separator();
            render_resource_summary(ui, &view);
        });
    }
}

fn render_node_tree_entry(
    ui: &mut egui::Ui,
    view: &ProjectView,
    id: NodeId,
    depth: usize,
    selected_node: &mut Option<NodeId>,
) {
    let Some(entry) = view.tree.nodes.get(&id) else {
        return;
    };

    ui.horizontal(|ui| {
        ui.add_space((depth as f32) * 14.0);
        let label = entry.path.0.last().map_or_else(
            || entry.path.to_string(),
            |segment| segment.name.to_string(),
        );
        let selected = *selected_node == Some(id);
        if ui
            .selectable_label(selected, format!("{label}  #{}", id.0))
            .clicked()
        {
            *selected_node = Some(id);
        }
    });

    for child in &entry.children {
        render_node_tree_entry(ui, view, *child, depth + 1, selected_node);
    }
}

fn render_node_slots(ui: &mut egui::Ui, view: &ProjectView, node_id: NodeId) {
    ui.heading("Slots");
    let mut rendered = false;

    for suffix in ["def", "state"] {
        let root = format!("node.{}.{}", node_id.0, suffix);
        let Some(data) = view.slots.roots.get(&root) else {
            continue;
        };
        let Some(shape) = view.slots.root_shapes.get(&root).copied() else {
            continue;
        };

        rendered = true;
        egui::CollapsingHeader::new(suffix)
            .default_open(suffix == "def")
            .show(ui, |ui| {
                render_slot_data(ui, &view.slots.registry, shape, data);
            });
    }

    if !rendered {
        ui.label("No slot roots for selected node.");
    }
}

fn render_slot_data(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    data: &SlotData,
) {
    match registry.get(&shape_id) {
        Some(shape) => render_slot_shape_data(ui, registry, shape, data),
        None => {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Missing shape {shape_id}"),
            );
            render_slot_data_fallback(ui, data);
        }
    }
}

fn render_slot_shape_data(
    ui: &mut egui::Ui,
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
    data: &SlotData,
) {
    match shape {
        SlotShape::Ref { id } => render_slot_data(ui, registry, *id, data),
        SlotShape::Unit { .. } => {
            if let SlotData::Unit { revision } = data {
                ui.label(format!("unit  rev {}", revision.0));
            } else {
                render_shape_mismatch(ui, "unit", data);
            }
        }
        SlotShape::Value { shape } => {
            if let SlotData::Value(value) = data {
                ui.horizontal_wrapped(|ui| {
                    ui.monospace(format_lp_value(value.value()));
                    ui.label(format!("rev {}", value.changed_at().0));
                    ui.label(format!("type {}", format_lp_type(&shape.ty)));
                    if shape.editor != Default::default() {
                        ui.label(format!("editor {:?}", shape.editor));
                    }
                });
            } else {
                render_shape_mismatch(ui, "value", data);
            }
        }
        SlotShape::Record { fields, .. } => {
            if let SlotData::Record(record) = data {
                ui.label(format!("fields rev {}", record.fields_revision.0));
                for (index, field) in fields.iter().enumerate() {
                    let Some(child) = record.fields.get(index) else {
                        ui.colored_label(
                            egui::Color32::LIGHT_RED,
                            format!("missing field {}", field.name.as_str()),
                        );
                        continue;
                    };
                    egui::CollapsingHeader::new(field.name.as_str())
                        .default_open(false)
                        .show(ui, |ui| {
                            render_slot_shape_data(ui, registry, &field.shape, child);
                        });
                }
            } else {
                render_shape_mismatch(ui, "record", data);
            }
        }
        SlotShape::Map { value, .. } => {
            if let SlotData::Map(map) = data {
                ui.label(format!(
                    "{} entries  keys rev {}",
                    map.entries.len(),
                    map.keys_revision.0
                ));
                for (key, child) in &map.entries {
                    egui::CollapsingHeader::new(format_slot_map_key(key))
                        .default_open(false)
                        .show(ui, |ui| {
                            render_slot_shape_data(ui, registry, value, child);
                        });
                }
            } else {
                render_shape_mismatch(ui, "map", data);
            }
        }
        SlotShape::Enum { variants, .. } => {
            if let SlotData::Enum(value) = data {
                ui.label(format!(
                    "variant {}  rev {}",
                    value.variant.as_str(),
                    value.variant_revision.0
                ));
                if let Some(variant) = variants.iter().find(|v| v.name == value.variant) {
                    render_slot_shape_data(ui, registry, &variant.shape, &value.data);
                } else {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("missing variant shape {}", value.variant.as_str()),
                    );
                    render_slot_data_fallback(ui, &value.data);
                }
            } else {
                render_shape_mismatch(ui, "enum", data);
            }
        }
        SlotShape::Option { some, .. } => {
            if let SlotData::Option(value) = data {
                ui.label(format!("presence rev {}", value.presence_revision.0));
                match &value.data {
                    Some(child) => render_slot_shape_data(ui, registry, some, child),
                    None => {
                        ui.monospace("none");
                    }
                }
            } else {
                render_shape_mismatch(ui, "option", data);
            }
        }
    }
}

fn render_shape_mismatch(ui: &mut egui::Ui, expected: &str, data: &SlotData) {
    ui.colored_label(
        egui::Color32::LIGHT_RED,
        format!("Shape/data mismatch: expected {expected}"),
    );
    render_slot_data_fallback(ui, data);
}

fn render_slot_data_fallback(ui: &mut egui::Ui, data: &SlotData) {
    ui.monospace(format!("{data:?}"));
}

fn render_resource_summary(ui: &mut egui::Ui, view: &ProjectView) {
    ui.heading("Resources");
    ui.label(format!(
        "{} summaries cached",
        view.resource_cache.summary_count()
    ));
}

fn format_slot_map_key(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => value.clone(),
        SlotMapKey::I32(value) => value.to_string(),
        SlotMapKey::U32(value) => value.to_string(),
    }
}

fn format_lp_value(value: &LpValue) -> String {
    match value {
        LpValue::String(value) => format!("{value:?}"),
        LpValue::I32(value) => value.to_string(),
        LpValue::U32(value) => value.to_string(),
        LpValue::F32(value) => format!("{value:.3}"),
        LpValue::Bool(value) => value.to_string(),
        LpValue::Vec2(value) => format!("[{:.3}, {:.3}]", value[0], value[1]),
        LpValue::Vec3(value) => format!("[{:.3}, {:.3}, {:.3}]", value[0], value[1], value[2]),
        LpValue::Vec4(value) => format!(
            "[{:.3}, {:.3}, {:.3}, {:.3}]",
            value[0], value[1], value[2], value[3]
        ),
        LpValue::IVec2(value) => format!("{value:?}"),
        LpValue::IVec3(value) => format!("{value:?}"),
        LpValue::IVec4(value) => format!("{value:?}"),
        LpValue::UVec2(value) => format!("{value:?}"),
        LpValue::UVec3(value) => format!("{value:?}"),
        LpValue::UVec4(value) => format!("{value:?}"),
        LpValue::BVec2(value) => format!("{value:?}"),
        LpValue::BVec3(value) => format!("{value:?}"),
        LpValue::BVec4(value) => format!("{value:?}"),
        LpValue::Mat2x2(value) => format!("{value:?}"),
        LpValue::Mat3x3(value) => format!("{value:?}"),
        LpValue::Mat4x4(value) => format!("{value:?}"),
        LpValue::Array(values) => format!("array[{}]", values.len()),
        LpValue::Struct { name, fields } => {
            format!(
                "{} struct[{}]",
                name.as_deref().unwrap_or("anonymous"),
                fields.len()
            )
        }
        LpValue::Resource(value) => format!("resource {:?}/{}", value.domain, value.id),
        LpValue::Product(value) => format!("product {value:?}"),
    }
}

fn format_lp_type(ty: &LpType) -> String {
    match ty {
        LpType::String => String::from("string"),
        LpType::I32 => String::from("i32"),
        LpType::U32 => String::from("u32"),
        LpType::F32 => String::from("f32"),
        LpType::Bool => String::from("bool"),
        LpType::Vec2 => String::from("vec2"),
        LpType::Vec3 => String::from("vec3"),
        LpType::Vec4 => String::from("vec4"),
        LpType::IVec2 => String::from("ivec2"),
        LpType::IVec3 => String::from("ivec3"),
        LpType::IVec4 => String::from("ivec4"),
        LpType::UVec2 => String::from("uvec2"),
        LpType::UVec3 => String::from("uvec3"),
        LpType::UVec4 => String::from("uvec4"),
        LpType::BVec2 => String::from("bvec2"),
        LpType::BVec3 => String::from("bvec3"),
        LpType::BVec4 => String::from("bvec4"),
        LpType::Mat2x2 => String::from("mat2x2"),
        LpType::Mat3x3 => String::from("mat3x3"),
        LpType::Mat4x4 => String::from("mat4x4"),
        LpType::Array(item, len) => format!("[{}; {len}]", format_lp_type(item)),
        LpType::List(item) => format!("list<{}>", format_lp_type(item)),
        LpType::Struct { name, fields } => format!(
            "{} struct[{}]",
            name.as_deref().unwrap_or("anonymous"),
            fields.len()
        ),
        LpType::Resource => String::from("resource"),
        LpType::Product(kind) => format!("product::{kind:?}"),
    }
}
