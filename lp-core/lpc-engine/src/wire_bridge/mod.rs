//! Conversions between model/wire shapes and shader runtime types (`lps-shared`).

mod lps_value_to_wire_value;
mod wire_type_to_lps_type;

pub use lps_value_to_wire_value::lps_value_f32_to_wire_value;
pub use wire_type_to_lps_type::wire_type_to_lps_type;
