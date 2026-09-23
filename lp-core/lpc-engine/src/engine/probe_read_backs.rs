//! Per-probe readback state for the render-product probe on a backend whose
//! render products stay GPU-resident.
//!
//! The browser GPU tier cannot block on a buffer map, so it answers a probe
//! with the bytes of the *previous* probe of the same product
//! ([`LatentReadBackSource`]). That needs the source, which the host injects
//! ([`super::Engine::set_latent_read_back`]), and somewhere to keep the
//! staging buffer between probes: one [`LatentReadBack`] per (product,
//! size, space, policy), least-recently-used first out. Device images never
//! inject a source, so on a device this whole store is one `None`.

use alloc::sync::Arc;
use alloc::vec::Vec;

use lp_gfx::{LatentReadBack, LatentReadBackSource};
use lpc_model::VisualProduct;

use crate::products::visual::{ConsumerPolicy, VisualSpace};

/// Read sites retained at once. A Studio lens probes each visible product
/// once per checked space, so this is "previews on screen" with room to
/// spare; past it the least recently probed site is dropped, and its next
/// probe starts over (one read with no bytes).
const MAX_PROBE_READ_BACKS: usize = 32;

/// What makes two probes the same read site: the same frame shape of the
/// same product, rendered the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProbeReadBackKey {
    pub(super) product: VisualProduct,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) space: VisualSpace,
    pub(super) policy: ConsumerPolicy,
}

/// The injected source and its retained read sites, most recently used at
/// the back.
pub(super) struct ProbeReadBacks {
    source: Arc<dyn LatentReadBackSource>,
    sites: Vec<(ProbeReadBackKey, LatentReadBack)>,
}

impl ProbeReadBacks {
    pub(super) fn new(source: Arc<dyn LatentReadBackSource>) -> Self {
        Self {
            source,
            sites: Vec::new(),
        }
    }

    /// The source, and the state for `key` — created empty on first use and
    /// moved to the most-recently-used end.
    ///
    /// No `expect` or indexing: this code is linked into every device image
    /// (the probe reaches it on a runtime branch), so it stays free of panic
    /// paths. `None` is unreachable in practice — the site was just pushed.
    pub(super) fn source_and_site(
        &mut self,
        key: ProbeReadBackKey,
    ) -> Option<(&dyn LatentReadBackSource, &mut LatentReadBack)> {
        let source = &*self.source;
        let sites = &mut self.sites;
        match sites.iter().position(|(existing, _)| *existing == key) {
            Some(index) => {
                if let Some(tail) = sites.get_mut(index..) {
                    tail.rotate_left(1);
                }
            }
            None => {
                if sites.len() >= MAX_PROBE_READ_BACKS {
                    sites.rotate_left(1);
                    sites.pop();
                }
                sites.push((key, LatentReadBack::new()));
            }
        }
        let (_, site) = sites.last_mut()?;
        Some((source, site))
    }

    /// Drop every retained site (and any read in flight): the textures they
    /// staged belong to a backend that was just replaced.
    pub(super) fn clear(&mut self) {
        self.sites.clear();
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.sites.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_is_reused_and_the_least_recent_is_evicted_at_the_cap() {
        let mut sites = ProbeReadBacks::new(Arc::new(NeverLands));
        for width in 0..MAX_PROBE_READ_BACKS as u32 {
            sites.source_and_site(key(width));
        }
        // Touch the oldest so the next-oldest is the one evicted.
        sites.source_and_site(key(0));
        assert_eq!(sites.len(), MAX_PROBE_READ_BACKS);
        sites.source_and_site(key(1000));
        assert_eq!(sites.len(), MAX_PROBE_READ_BACKS);
        assert!(sites.sites.iter().any(|(k, _)| *k == key(0)));
        assert!(!sites.sites.iter().any(|(k, _)| *k == key(1)));
        sites.clear();
        assert_eq!(sites.len(), 0);
    }

    struct NeverLands;

    impl LatentReadBackSource for NeverLands {
        fn read_back_latent(
            &self,
            _texture: &lp_gfx::TextureHandle,
            _state: &mut LatentReadBack,
            _tag: u64,
            _out: &mut Vec<u8>,
        ) -> Result<Option<u64>, lp_gfx::GfxError> {
            Ok(None)
        }
    }

    fn key(width: u32) -> ProbeReadBackKey {
        ProbeReadBackKey {
            product: VisualProduct::default(),
            width,
            height: 1,
            space: VisualSpace::TwoD,
            policy: ConsumerPolicy::AUTO,
        }
    }
}
