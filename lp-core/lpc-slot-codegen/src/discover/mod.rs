mod derive;
mod registered_shapes;
mod rust_files;
mod slot_records;
mod slot_views;
mod type_path;

pub(crate) use registered_shapes::discover_static_registered_shapes;
pub(crate) use slot_records::discover_static_slot_records;
pub(crate) use slot_views::discover_static_slot_views;
