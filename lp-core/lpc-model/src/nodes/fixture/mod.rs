pub mod brightness;
pub mod diagnostic_mode;
pub mod fixture_def;
pub mod fixture_state;
pub mod lamp_presets;
pub mod lamp_type;
pub mod mapping;
pub mod mapping_points;
pub mod power;
pub mod power_model;
pub mod resolved_mapping;
pub mod sampling;
pub mod visual_consumer_space;

pub use crate::slot_views::{FixtureDefView, FixtureStateView};
pub use brightness::Brightness;
pub use diagnostic_mode::FixtureDiagnosticMode;
pub use fixture_def::{ColorOrder, FixtureDef};
pub use fixture_state::FixtureState;
pub use lamp_presets::{LampPreset, PowerProvenance, preset_for};
pub use lamp_type::LampType;
pub use mapping::{MappingConfig, PatchConfig, PathSpec};
pub use mapping_points::{
    MappingPoint, for_each_mapping_point, generate_mapping_points, mapping_point_count,
};
pub use power::FixturePower;
pub use power_model::PowerModel;
pub use resolved_mapping::{MappingRef, ResolvedMappingCompact, ResolvedSpan};
pub use sampling::FixtureSamplingConfig;
pub use visual_consumer_space::{ConsumerCell2, VisualConsumerSpace};
