//! Per-probe readback state for the render-product probe on a backend whose
//! render products stay GPU-resident.
//!
//! The browser GPU tier cannot block on a buffer map, so it answers a probe
//! with the bytes of the *previous* probe of the same product
//! ([`lp_gfx::LpGraphics::read_back_latent`]). That needs somewhere to keep
//! the staging buffer between probes: here, one [`LatentReadBack`] per
//! (product, size, space, policy), least-recently-used first out. A CPU
//! backend never reaches this store — its products carry host bytes — so a
//! device image pays for the code and nothing else.

use alloc::vec::Vec;

use lp_gfx::LatentReadBack;
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

/// The retained read sites, most recently used at the back.
#[derive(Debug, Default)]
pub(super) struct ProbeReadBacks {
    sites: Vec<(ProbeReadBackKey, LatentReadBack)>,
}

impl ProbeReadBacks {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// The state for `key`, created empty on first use and moved to the
    /// most-recently-used end.
    pub(super) fn site(&mut self, key: ProbeReadBackKey) -> &mut LatentReadBack {
        let site = match self.sites.iter().position(|(existing, _)| *existing == key) {
            Some(index) => self.sites.remove(index),
            None => {
                if self.sites.len() >= MAX_PROBE_READ_BACKS {
                    self.sites.remove(0);
                }
                (key, LatentReadBack::new())
            }
        };
        self.sites.push(site);
        &mut self.sites.last_mut().expect("site was just pushed").1
    }

    /// Drop every retained site (and any read in flight) — memory
    /// pressure, or a project that went away.
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
        let mut sites = ProbeReadBacks::new();
        for width in 0..MAX_PROBE_READ_BACKS as u32 {
            sites.site(key(width));
        }
        // Touch the oldest so the next-oldest is the one evicted.
        sites.site(key(0));
        assert_eq!(sites.len(), MAX_PROBE_READ_BACKS);
        sites.site(key(1000));
        assert_eq!(sites.len(), MAX_PROBE_READ_BACKS);
        assert!(sites.sites.iter().any(|(k, _)| *k == key(0)));
        assert!(!sites.sites.iter().any(|(k, _)| *k == key(1)));
        sites.clear();
        assert_eq!(sites.len(), 0);
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
