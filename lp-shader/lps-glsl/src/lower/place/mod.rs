mod dynamic;
mod layout;
mod path;
mod read;
mod write;

pub(super) use read::try_read_place_direct;
pub(super) use write::try_assign_place_direct;
