//! The place seam: kind and capacity.

use crate::app::library::{LibraryError, LibraryStore, PackageSummary};

/// What sort of place this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceKind {
    /// The local library — the source of truth.
    Library,
    /// An ephemeral simulator runtime (a device with no memory — D19).
    SimRuntime,
    /// A physical device (serial today, networked later).
    Device,
}

/// Capacity and kind, the facts the UI shapes itself around (D18: the
/// device card IS the slot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceDescriptor {
    pub kind: PlaceKind,
    /// `None` = unbounded (the library); `Some(1)` = single-slot runtimes.
    pub capacity: Option<usize>,
}

/// A place a project can live. Grown deliberately small — see module docs.
pub trait Place {
    fn descriptor(&self) -> PlaceDescriptor;
}

/// The library as a place.
pub struct LibraryPlace {
    pub store: LibraryStore,
}

impl LibraryPlace {
    pub fn list(&self) -> Result<Vec<PackageSummary>, LibraryError> {
        self.store.list()
    }
}

impl Place for LibraryPlace {
    fn descriptor(&self) -> PlaceDescriptor {
        PlaceDescriptor {
            kind: PlaceKind::Library,
            capacity: None,
        }
    }
}
