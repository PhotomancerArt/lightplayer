//! Pre-computed texture-to-fixture mapping utilities

pub mod accumulation;
pub mod entry;
pub mod overlap;
pub mod points;
pub mod precompute;
pub mod sampling;
pub mod structure;

// Re-export public API
pub use accumulation::{accumulate_from_mapping, initialize_channel_accumulators};
pub use entry::{CHANNEL_SKIP, PixelMappingEntry};
pub use overlap::circle::circle_pixel_overlap;
pub use points::{MappingPoint, generate_mapping_points};
pub use precompute::compute_mapping;
pub use sampling::{TextureSampler, create_sampler};
pub use structure::PrecomputedMapping;
