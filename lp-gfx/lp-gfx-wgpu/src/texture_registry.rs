//! Backend-owned registry mapping texture ids to live wgpu views.
//!
//! The engine-facing texture uniform currency is
//! `LpsValueF32::Texture2D(LpsTexture2DValue)` — on the CPU tier its
//! `descriptor.ptr` is a guest pointer. On the GPU tier there is no guest:
//! [`crate::GpuGraphics`] mints an opaque nonzero id per texture allocation
//! and carries it in the `ptr` lane
//! ([`crate::GpuGraphics::texture_uniform_value`]). At render time the
//! shader resolves the id back to the wgpu view through this registry and
//! binds it — the descriptor "leg" of the uniform tree becomes a bind-group
//! entry.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use lps_shared::TextureStorageFormat;

/// Live GPU texture facts behind a minted id.
#[derive(Clone)]
pub(crate) struct RegisteredTexture {
    pub(crate) view: wgpu::TextureView,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TextureStorageFormat,
}

/// Id → texture map shared by the backend facade and every compiled shader.
pub(crate) struct TextureRegistry {
    /// Next id to mint; starts at 1 so `0` is never a valid texture id.
    next_id: AtomicU32,
    entries: Mutex<BTreeMap<u32, RegisteredTexture>>,
}

impl TextureRegistry {
    pub(crate) fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1),
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Register a texture and mint its id.
    pub(crate) fn register(&self, entry: RegisteredTexture) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.entries
            .lock()
            .expect("texture registry lock")
            .insert(id, entry);
        id
    }

    /// Drop a registration (called when the owning handle is freed).
    pub(crate) fn unregister(&self, id: u32) {
        self.entries
            .lock()
            .expect("texture registry lock")
            .remove(&id);
    }

    /// Look up a live texture by id (clones the view; wgpu views are
    /// internally reference-counted).
    pub(crate) fn get(&self, id: u32) -> Option<RegisteredTexture> {
        self.entries
            .lock()
            .expect("texture registry lock")
            .get(&id)
            .cloned()
    }
}
