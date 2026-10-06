//! **What a build is, without its bytes** (DS5): the identity, the build
//! hash, and each piece's hash and length — everything [`crate::decide()`]
//! reads. A card computes a board's standing on every view; it must not load
//! a build's ~5 MB of core and engine to say "Update available".
//!
//! [`HostBuild::facts`](crate::HostBuild::facts) gives a held build's facts;
//! [`HostBuildFacts::from_parts`] builds them from what a build's manifest
//! states (Studio reads them from its `ota-manifest.json`; the bytes are
//! fetched only when an update runs).

use lpc_update::build_id::build_hash;

use crate::host_build::HostIdentity;

/// One piece, known by hash and length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostPieceFacts {
    /// By the hash rules (`lpc_update::hash_rules`): the core's SHA-256 over
    /// its bytes, the engine's by the engine hash rule.
    pub sha256: [u8; 32],
    /// The piece's length, as the offer states it.
    pub len: u32,
}

/// A build's facts. See the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostBuildFacts {
    pub identity: HostIdentity,
    pub core: HostPieceFacts,
    pub engine: HostPieceFacts,
    build_hash: u32,
}

impl HostBuildFacts {
    /// The facts of a build the host knows of but need not hold: its
    /// identity and each piece's hash and length. The build hash is
    /// computed here from the build id.
    #[must_use]
    pub fn from_parts(
        identity: HostIdentity,
        core: HostPieceFacts,
        engine: HostPieceFacts,
    ) -> Self {
        let build_hash = build_hash(identity.build_id.as_bytes());
        Self {
            identity,
            core,
            engine,
            build_hash,
        }
    }

    /// The build hash the board's records key on (`refusedBuild`,
    /// `transfer.buildHash`).
    #[must_use]
    pub fn build_hash(&self) -> u32 {
        self.build_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    use crate::host_build::HostBuild;

    #[test]
    fn a_held_builds_facts_equal_the_facts_from_its_parts() {
        let identity = HostIdentity {
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.06-1".into(),
            build_id: "2026.10.06-1+abcdefabcdef".into(),
            wire_proto: 36,
            layout: 1,
            min_loader: 1,
        };
        let held =
            HostBuild::from_parts(identity.clone(), vec![1; 5000], vec![2; 9000], None, None)
                .unwrap();
        let facts = HostBuildFacts::from_parts(
            identity,
            HostPieceFacts {
                sha256: held.core.sha256,
                len: 5000,
            },
            HostPieceFacts {
                sha256: held.engine.sha256,
                len: 9000,
            },
        );
        assert_eq!(held.facts(), facts);
        assert_eq!(facts.build_hash(), held.build_hash());
    }
}
