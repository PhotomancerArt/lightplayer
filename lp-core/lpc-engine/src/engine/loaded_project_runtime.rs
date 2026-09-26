//! Loaded project runtime helper returned by [`super::ProjectLoader`].

use core::ops::{Deref, DerefMut};

use lpc_registry::ProjectRegistry;
use lpfs::LpFs;

use super::{Engine, EngineError};

/// A loaded runtime projection paired with the registry state it was built from.
///
/// `Engine` is intentionally only the runtime projection. This helper exists
/// for direct embedders and tests that load a project through `lpc-engine`
/// without the higher-level server `Project` wrapper.
pub struct LoadedProjectRuntime {
    engine: Engine,
    registry: ProjectRegistry,
}

impl LoadedProjectRuntime {
    pub(crate) fn new(engine: Engine, registry: ProjectRegistry) -> Self {
        Self { engine, registry }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn registry(&self) -> &ProjectRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut ProjectRegistry {
        &mut self.registry
    }

    pub fn into_parts(self) -> (Engine, ProjectRegistry) {
        (self.engine, self.registry)
    }

    /// Disjoint mutable borrows, for the write paths that take both
    /// ([`Engine::apply_project_changes`], [`Engine::apply_residency`]).
    pub fn split_mut(&mut self) -> (&mut Engine, &mut ProjectRegistry) {
        (&mut self.engine, &mut self.registry)
    }

    /// Disjoint borrows for read paths that resolve against the registry.
    pub fn read_parts(&mut self) -> (&mut Engine, &ProjectRegistry) {
        (&mut self.engine, &self.registry)
    }

    /// Tick the engine. This helper owns no filesystem, so it cannot run the
    /// pre-tick residency step itself: an embedder whose project has
    /// playlists calls [`Self::apply_residency`] first, or uses
    /// [`Self::tick_with_residency`]. The product's edges tick through
    /// `lpa-server`'s `Project::tick`, which always does.
    pub fn tick(&mut self, delta_ms: u32) -> Result<(), EngineError> {
        self.engine.tick(&self.registry, delta_ms)
    }

    /// The pre-tick residency step ([`Engine::apply_residency`]) against this
    /// runtime's own registry.
    pub fn apply_residency(
        &mut self,
        fs: &dyn LpFs,
    ) -> Result<super::ResidencyApplied, EngineError> {
        self.engine.apply_residency(fs, &mut self.registry)
    }

    /// [`Self::apply_residency`], then [`Self::tick`] — the order every edge
    /// uses.
    pub fn tick_with_residency(&mut self, fs: &dyn LpFs, delta_ms: u32) -> Result<(), EngineError> {
        self.apply_residency(fs)?;
        self.tick(delta_ms)
    }

    #[cfg(test)]
    pub(crate) fn resolve_with_engine_host(
        &mut self,
        key: crate::dataflow::resolver::QueryKey,
        log_level: crate::dataflow::resolver::ResolveLogLevel,
    ) -> Result<
        (
            crate::dataflow::resolver::Production,
            crate::dataflow::resolver::ResolveTrace,
        ),
        crate::dataflow::resolver::SessionResolveError,
    > {
        super::resolve_with_engine_host(&mut self.engine, &self.registry, key, log_level)
    }

    #[cfg(test)]
    pub(crate) fn render_texture_for_test(
        &mut self,
        product: crate::products::visual::VisualProduct,
        request: &crate::products::visual::RenderTextureRequest,
    ) -> Result<
        crate::products::visual::TextureRenderProduct,
        crate::dataflow::resolver::SessionResolveError,
    > {
        self.engine
            .render_texture_for_test(&self.registry, product, request)
    }
}

impl Deref for LoadedProjectRuntime {
    type Target = Engine;

    fn deref(&self) -> &Self::Target {
        &self.engine
    }
}

impl DerefMut for LoadedProjectRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.engine
    }
}
