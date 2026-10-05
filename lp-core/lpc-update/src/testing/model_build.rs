//! A build, as the model knows it: a core image and an engine image with
//! the model's own engine header.
//!
//! **A model, not the split image's formats.** The engine's first 12 bytes
//! are the model's header: `"FEH1"`, the commit word `"LPOK"` (as shipped:
//! committed), and the engine's length (u32 LE). Everything else is
//! synthetic content, chosen to look like firmware to a compressor: a block
//! that repeats every 10 KiB with sparse changes (so encoding 1's dictionary
//! pays), and an incompressible stretch (so some chunks have no compressed
//! form).

use alloc::string::String;
use alloc::vec::Vec;

use crate::build_id::{BUILD_ID_LEN, build_hash, build_id_field};
use crate::hash_rules::{core_sha256, engine_sha256};

/// The model engine header's magic.
pub const MODEL_ENGINE_MAGIC: [u8; 4] = *b"FEH1";
/// The model engine header's commit word, at bytes 4..8.
pub const MODEL_COMMIT_WORD: [u8; 4] = *b"LPOK";
/// Where the commit word sits in the engine's first sector.
pub const MODEL_COMMIT_AT: usize = 4;

/// One build: its identity and its two pieces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelBuild {
    pub version: String,
    pub build_id: String,
    pub core: Vec<u8>,
    pub engine: Vec<u8>,
    /// The engine crashes whenever it runs (E10).
    pub crashes: bool,
}

impl ModelBuild {
    /// A synthetic build: `version`, a 12-digit commit made from `seed`, and
    /// pieces of the given lengths whose bytes depend on `seed`.
    #[must_use]
    pub fn synthetic(version: &str, seed: u32, core_len: usize, engine_len: usize) -> Self {
        let commit = alloc::format!("{:012x}", u64::from(seed).wrapping_mul(0x9e37_79b9_7f4a));
        let build_id = alloc::format!("{version}+{}", &commit[commit.len() - 12..]);
        let core = content(seed, core_len);
        let mut engine = content(seed ^ 0x5a5a_5a5a, engine_len);
        if engine.len() >= 12 {
            engine[0..4].copy_from_slice(&MODEL_ENGINE_MAGIC);
            engine[4..8].copy_from_slice(&MODEL_COMMIT_WORD);
            engine[8..12].copy_from_slice(&(engine_len as u32).to_le_bytes());
        }
        Self {
            version: version.into(),
            build_id,
            core,
            engine,
            crashes: false,
        }
    }

    /// The same build, whose engine crashes whenever it runs.
    #[must_use]
    pub fn crashing(mut self) -> Self {
        self.crashes = true;
        self
    }

    #[must_use]
    pub fn core_sha256(&self) -> [u8; 32] {
        core_sha256(&self.core)
    }

    #[must_use]
    pub fn engine_sha256(&self) -> [u8; 32] {
        engine_sha256(&self.engine)
    }

    /// The build id field (zero-padded).
    #[must_use]
    pub fn build_id_field(&self) -> [u8; BUILD_ID_LEN] {
        build_id_field(self.build_id.as_bytes()).unwrap_or([0; BUILD_ID_LEN])
    }

    /// The build hash the records key on.
    #[must_use]
    pub fn build_hash(&self) -> u32 {
        build_hash(self.build_id.as_bytes())
    }
}

/// Firmware-looking bytes: a 10 KiB block repeated with sparse changes,
/// then (from 3/4 of the way) an incompressible stretch.
fn content(seed: u32, len: usize) -> Vec<u8> {
    let mut s = seed | 1;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    let block: Vec<u8> = (0..10 * 1024).map(|_| (next() % 7) as u8 * 31).collect();
    let noisy_from = len * 3 / 4;
    (0..len)
        .map(|i| {
            if i >= noisy_from {
                next() as u8
            } else if i % 97 == 0 {
                next() as u8
            } else {
                block[i % block.len()]
            }
        })
        .collect()
}
