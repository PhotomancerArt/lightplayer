//! The bake cache: resolved palette → texture, keyed by value hash.
//!
//! A palette uniform's texels are a pure function of what was resolved for it
//! this tick — the gradient (or the two gradients and a quantized mix). So
//! the texture that carries them is addressed by a **hash of that value**,
//! not by the slot that asked for it. Two uniforms resolving to the same
//! palette share one texture; a panel drag re-bakes at most once per tick,
//! because the second frame of a held value hashes the same; and a scrub back
//! onto a mix the cache still holds costs nothing at all.
//!
//! # Why the strips are reused in place
//!
//! Every palette bake is exactly [`PALETTE_BAKE_WIDTH`] × 1
//! [`PALETTE_BAKE_FORMAT`], so a miss does not need a new allocation: it
//! overwrites the least-recently-used strip with
//! [`LpGraphics::write_texture`] and re-keys it. In steady state — including
//! a fade, which mints a new key every quantization step — the cache
//! allocates nothing at all. The uniform value survives the rewrite
//! (`write_texture` does not move the backing), which is what makes that
//! sound.
//!
//! # Capacity
//!
//! One strip per live palette uniform plus **one spare**. The spare is what
//! makes a fade's previous quantization step still resident, so scrubbing a
//! clock back and forth across a dissolve hits rather than re-bakes; more
//! than one spare buys progressively less, and each strip is
//! [`PALETTE_BAKE_BYTES`] of device RAM that a dome-scale project would
//! rather spend on LEDs.
//!
//! # Collisions
//!
//! The key is a 64-bit FNV-1a over the resolved value's bit patterns, and a
//! hit is taken on the hash alone rather than re-comparing the gradients.
//! Storing the inputs to confirm would roughly double the cache's resident
//! size to defend against a 2⁻⁶⁴ event; the hash is the identity, which is
//! also what "keyed by value hash" means everywhere else in this engine.

use alloc::vec::Vec;

use lp_gfx::{GfxError, LpGraphics, TextureHandle};
use lpc_model::Gradient;
use lps_shared::LpsValueF32;

use crate::color::{
    PALETTE_BAKE_BYTES, PALETTE_BAKE_FORMAT, PALETTE_BAKE_WIDTH, bake_gradient_into,
    bake_gradient_mix_into,
};

use super::palette_eval::PALETTE_MIX_STEPS;

/// Strips kept beyond the number of live palette uniforms. See the module
/// docs — one, deliberately.
const PALETTE_BAKE_SPARE_STRIPS: usize = 1;

/// Identity of one baked strip: the hash of everything that went into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PaletteBakeKey(u64);

/// What a palette uniform resolved to this tick, before it is a texture.
///
/// Borrowed rather than owned: the gradients live in the slot def or in the
/// resolved channel value, and a bake that hits the cache must not have paid
/// to clone them.
#[derive(Clone, Copy, Debug)]
pub struct PaletteBake<'a> {
    /// The gradient held, or the one being faded *from*.
    pub from: &'a Gradient,
    /// The gradient being faded *to*; `from` when nothing is fading.
    pub to: &'a Gradient,
    /// Quantized cross-fade position in `0..=PALETTE_MIX_STEPS`. Zero is a
    /// single-gradient bake and never touches `to`.
    pub mix_steps: u32,
}

impl PaletteBake<'_> {
    /// One gradient, held.
    #[must_use]
    pub fn held(gradient: &Gradient) -> PaletteBake<'_> {
        PaletteBake {
            from: gradient,
            to: gradient,
            mix_steps: 0,
        }
    }

    /// This bake's cache identity.
    ///
    /// A single-gradient bake hashes **only** `from` and a zero mix, so the
    /// moment a fade completes it lands back on the very key the plain static
    /// bake of that entry already has.
    #[must_use]
    pub fn key(&self) -> PaletteBakeKey {
        let mut hash = FNV_OFFSET;
        hash = hash_gradient(hash, self.from);
        if self.mix_steps > 0 {
            hash = hash_gradient(hash, self.to);
            hash = hash_u32(hash, self.mix_steps);
        } else {
            hash = hash_u32(hash, 0);
        }
        PaletteBakeKey(hash)
    }

    /// Write this bake's texels.
    fn write(&self, out: &mut [u8]) {
        if self.mix_steps == 0 {
            bake_gradient_into(self.from, out);
        } else {
            bake_gradient_mix_into(
                self.from,
                self.to,
                self.mix_steps as f32 / PALETTE_MIX_STEPS as f32,
                out,
            );
        }
    }
}

/// One node's palette strips, addressed by [`PaletteBakeKey`].
pub struct PaletteBakeCache {
    strips: Vec<Strip>,
    /// Monotonic touch counter — cheaper than threading the frame revision
    /// through, and the only ordering the LRU needs.
    clock: u64,
    /// Scratch the bake is written into before it is uploaded. One buffer for
    /// the whole cache: a bake is never in flight across two calls.
    scratch: Vec<u8>,
}

struct Strip {
    key: PaletteBakeKey,
    texture: TextureHandle,
    uniform: LpsValueF32,
    touched: u64,
}

impl PaletteBakeCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            strips: Vec::new(),
            clock: 0,
            scratch: Vec::new(),
        }
    }

    /// Whether `key` is already resident — the produce-side question, asked
    /// so a tick can skip baking texels it would only throw away.
    #[must_use]
    pub fn contains(&self, key: PaletteBakeKey) -> bool {
        self.strips.iter().any(|strip| strip.key == key)
    }

    /// The uniform value for `bake`, baking and uploading it on a miss.
    ///
    /// `live_slots` is how many palette uniforms this node has; it sets the
    /// capacity (see the module docs) and is passed per call so a node whose
    /// uniform set changes does not need the cache rebuilt.
    pub fn uniform_for(
        &mut self,
        graphics: &dyn LpGraphics,
        bake: &PaletteBake<'_>,
        live_slots: usize,
    ) -> Result<LpsValueF32, GfxError> {
        let key = bake.key();
        self.clock = self.clock.wrapping_add(1);
        if let Some(strip) = self.strips.iter_mut().find(|strip| strip.key == key) {
            strip.touched = self.clock;
            return Ok(strip.uniform.clone());
        }

        self.scratch.resize(PALETTE_BAKE_BYTES, 0);
        bake.write(&mut self.scratch);

        let capacity = live_slots.max(1) + PALETTE_BAKE_SPARE_STRIPS;
        if self.strips.len() < capacity {
            let texture = graphics.create_texture(
                PALETTE_BAKE_WIDTH,
                1,
                PALETTE_BAKE_FORMAT,
                &self.scratch,
            )?;
            let uniform = graphics.texture_uniform_value(&texture)?;
            self.strips.push(Strip {
                key,
                texture,
                uniform: uniform.clone(),
                touched: self.clock,
            });
            return Ok(uniform);
        }

        // Full: overwrite the least-recently-touched strip in place. The
        // uniform value is unchanged by a write, so it is re-read rather than
        // rebuilt only to keep this the single place that knows that.
        let victim = self.least_recently_touched();
        let strip = &mut self.strips[victim];
        graphics.write_texture(&mut strip.texture, &self.scratch)?;
        strip.key = key;
        strip.touched = self.clock;
        strip.uniform = graphics.texture_uniform_value(&strip.texture)?;
        Ok(strip.uniform.clone())
    }

    /// Drop every strip, releasing the textures.
    ///
    /// Memory pressure and node teardown; the next tick re-bakes what it
    /// still needs, which is exactly the rebuildable-cache contract that lets
    /// this be dropped at all.
    pub fn clear(&mut self) {
        self.strips.clear();
        self.scratch = Vec::new();
    }

    /// Resident strips — a test and diagnostics affordance.
    #[must_use]
    pub fn len(&self) -> usize {
        self.strips.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strips.is_empty()
    }

    fn least_recently_touched(&self) -> usize {
        let mut victim = 0;
        for (index, strip) in self.strips.iter().enumerate() {
            if strip.touched < self.strips[victim].touched {
                victim = index;
            }
        }
        victim
    }
}

impl Default for PaletteBakeCache {
    fn default() -> Self {
        Self::new()
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn hash_u32(hash: u64, value: u32) -> u64 {
    let mut hash = hash;
    for byte in value.to_le_bytes() {
        hash = (hash ^ byte as u64).wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Hash a gradient by the bits that reach a texel.
///
/// Float **bit patterns**, not values: two gradients whose stops differ only
/// in the sign of a zero bake identically, and hashing them apart costs one
/// extra bake once. Hashing them together would need `f32` equality, which is
/// not reflexive over `NaN` — and a `NaN` stop position is exactly the case
/// the bake path is written to survive.
fn hash_gradient(hash: u64, gradient: &Gradient) -> u64 {
    let mut hash = hash_u32(hash, gradient.space.as_i32() as u32);
    hash = hash_u32(hash, gradient.method.as_i32() as u32);
    hash = hash_u32(hash, gradient.stops.len() as u32);
    for stop in &gradient.stops {
        hash = hash_u32(hash, stop.at.to_bits());
        for lane in stop.c {
            hash = hash_u32(hash, lane.to_bits());
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::sync::Arc;
    use alloc::vec;
    use lpc_model::{Colorspace, GradientStop, InterpMethod};

    use super::*;
    use crate::nodes::shader::palette_eval::palette_cycle_position;
    use lpc_model::GradientConfig;

    #[test]
    fn a_held_bake_and_a_completed_fade_share_one_key() {
        let red = solid([1.0, 0.0, 0.0]);
        let blue = solid([0.0, 0.0, 1.0]);

        // A fade that has not started is the `from` entry held...
        assert_eq!(
            PaletteBake {
                from: &red,
                to: &blue,
                mix_steps: 0,
            }
            .key(),
            PaletteBake::held(&red).key()
        );
        // ...and a fade at full mix is NOT, because its texels are a blend
        // that happens to equal `to` only if the two agree everywhere.
        assert_ne!(
            PaletteBake {
                from: &red,
                to: &blue,
                mix_steps: PALETTE_MIX_STEPS,
            }
            .key(),
            PaletteBake::held(&blue).key()
        );
    }

    #[test]
    fn the_key_separates_every_input_that_changes_a_texel() {
        let red = solid([1.0, 0.0, 0.0]);
        let blue = solid([0.0, 0.0, 1.0]);
        let red_stepped = Gradient {
            method: InterpMethod::Step,
            ..red.clone()
        };
        let red_oklab = Gradient {
            space: Colorspace::Oklab,
            ..red.clone()
        };

        let base = PaletteBake::held(&red).key();
        assert_ne!(base, PaletteBake::held(&blue).key(), "stops");
        assert_ne!(base, PaletteBake::held(&red_stepped).key(), "method");
        assert_ne!(base, PaletteBake::held(&red_oklab).key(), "space");
        assert_ne!(
            PaletteBake {
                from: &red,
                to: &blue,
                mix_steps: 8,
            }
            .key(),
            PaletteBake {
                from: &red,
                to: &blue,
                mix_steps: 9,
            }
            .key(),
            "mix"
        );
        // The same value hashes the same however many times it is asked.
        assert_eq!(base, PaletteBake::held(&solid([1.0, 0.0, 0.0])).key());
    }

    #[test]
    fn a_swapped_fade_direction_is_a_different_bake() {
        let red = solid([1.0, 0.0, 0.0]);
        let blue = solid([0.0, 0.0, 1.0]);

        assert_ne!(
            PaletteBake {
                from: &red,
                to: &blue,
                mix_steps: 16,
            }
            .key(),
            PaletteBake {
                from: &blue,
                to: &red,
                mix_steps: 16,
            }
            .key()
        );
    }

    /// A held palette costs one texture, once — the second tick of an
    /// unchanged value neither bakes nor allocates.
    #[test]
    fn a_held_palette_allocates_one_strip_and_reuses_it() {
        let graphics = graphics();
        let mut cache = PaletteBakeCache::new();
        let red = solid([1.0, 0.0, 0.0]);

        let first = cache
            .uniform_for(graphics.as_ref(), &PaletteBake::held(&red), 1)
            .expect("bake");
        assert_eq!(cache.len(), 1);

        for _ in 0..64 {
            let again = cache
                .uniform_for(graphics.as_ref(), &PaletteBake::held(&red), 1)
                .expect("bake");
            assert!(again.eq(&first), "a held palette keeps its strip");
        }
        assert_eq!(cache.len(), 1, "and never allocates a second one");
    }

    /// Two uniforms resolving to the same palette get the **same** strip —
    /// the point of keying on the value rather than on the slot.
    #[test]
    fn two_uniforms_on_one_palette_share_a_strip() {
        let graphics = graphics();
        let mut cache = PaletteBakeCache::new();
        // Distinct `Gradient` values that happen to be equal, as two slots
        // resolving the same channel would produce.
        let a = solid([0.0, 1.0, 0.0]);
        let b = solid([0.0, 1.0, 0.0]);

        let first = cache
            .uniform_for(graphics.as_ref(), &PaletteBake::held(&a), 2)
            .expect("bake");
        let second = cache
            .uniform_for(graphics.as_ref(), &PaletteBake::held(&b), 2)
            .expect("bake");

        assert!(first.eq(&second));
        assert_eq!(cache.len(), 1);
    }

    /// Capacity is honoured, and a full cache reuses strips in place rather
    /// than growing — so a fade, which mints a key per quantization step,
    /// allocates nothing after the first two.
    #[test]
    fn a_full_cache_recycles_its_strips_instead_of_growing() {
        let graphics = graphics();
        let mut cache = PaletteBakeCache::new();
        let from = solid([1.0, 0.0, 0.0]);
        let to = solid([0.0, 0.0, 1.0]);

        for mix_steps in 0..=PALETTE_MIX_STEPS {
            cache
                .uniform_for(
                    graphics.as_ref(),
                    &PaletteBake {
                        from: &from,
                        to: &to,
                        mix_steps,
                    },
                    1,
                )
                .expect("bake");
        }

        assert_eq!(
            cache.len(),
            2,
            "one live palette uniform plus one spare, however many mixes ran"
        );
    }

    /// Clearing releases every strip; the next request rebuilds.
    #[test]
    fn clearing_releases_the_strips_and_the_next_bake_rebuilds() {
        let graphics = graphics();
        let mut cache = PaletteBakeCache::new();
        let red = solid([1.0, 0.0, 0.0]);

        cache
            .uniform_for(graphics.as_ref(), &PaletteBake::held(&red), 1)
            .expect("bake");
        assert!(!cache.is_empty());

        cache.clear();
        assert!(cache.is_empty());

        cache
            .uniform_for(graphics.as_ref(), &PaletteBake::held(&red), 1)
            .expect("re-bake");
        assert_eq!(cache.len(), 1);
    }

    /// **Perf sample** (reported, never gated): what one dome-scale frame of
    /// palette work costs.
    ///
    /// A strip is 256 texels whatever the fixture is, so the bake cost does
    /// not scale with LED count — which is the finding. What a 30k-LED dome
    /// pays per frame is one *cache lookup* unless the resolved value moved.
    /// The number that matters is therefore how often a cycling palette
    /// misses, and the mix quantization is what bounds it: a 4-entry set on
    /// 4 s steps with a 1 s fade, sampled at 60 fps for a full 16 s pass,
    /// re-bakes only when the entry or the quantized mix changes.
    #[test]
    fn dome_scale_palette_cost_is_bounded_by_the_mix_quantization() {
        use std::time::Instant;

        let graphics = graphics();
        let mut cache = PaletteBakeCache::new();
        let config = GradientConfig::Cycle {
            set: alloc::vec![
                solid([1.0, 0.0, 0.0]),
                solid([0.0, 1.0, 0.0]),
                solid([0.0, 0.0, 1.0]),
                solid([1.0, 1.0, 0.0]),
            ],
            step_seconds: 4.0,
            fade_seconds: 1.0,
        };
        let period = config.full_cycle_seconds();
        let frames = (period * 60.0) as u32;

        // Cold bake cost, measured on its own.
        let mut strip = vec![0u8; PALETTE_BAKE_BYTES];
        let started = Instant::now();
        for _ in 0..1000 {
            PaletteBake::held(config.gradients().first().expect("entry")).write(&mut strip);
        }
        let per_bake_us = started.elapsed().as_secs_f64() * 1000.0;

        // A full pass at 60 fps, counting misses.
        let mut misses = 0u32;
        let started = Instant::now();
        for frame in 0..frames {
            let phase = frame as f32 / frames as f32;
            let position = palette_cycle_position(&config, phase);
            let gradients = config.gradients();
            let bake = PaletteBake {
                from: &gradients[position.from],
                to: &gradients[position.to],
                mix_steps: position.mix_steps,
            };
            if !cache.contains(bake.key()) {
                misses += 1;
            }
            cache
                .uniform_for(graphics.as_ref(), &bake, 1)
                .expect("bake");
        }
        let per_frame_us = started.elapsed().as_secs_f64() * 1_000_000.0 / frames as f64;

        std::eprintln!(
            "[palette perf] bake {per_bake_us:.1} us/strip; {frames} frames of a \
             {period} s cycle: {misses} bakes ({:.1}% hit), {per_frame_us:.2} us/frame; \
             resident {} strips = {} B",
            100.0 * (1.0 - misses as f64 / frames as f64),
            cache.len(),
            cache.len() * PALETTE_BAKE_BYTES,
        );

        // The only assertion: quantization caps the fade's bakes. Four steps
        // plus at most `PALETTE_MIX_STEPS` fade positions each.
        assert!(
            misses <= 4 * (PALETTE_MIX_STEPS + 1),
            "{misses} bakes in one pass is more than quantization allows"
        );
        assert!(misses < frames, "the cache must hit at least sometimes");
    }

    fn graphics() -> Arc<dyn LpGraphics> {
        Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))
    }

    fn solid(c: [f32; 3]) -> Gradient {
        Gradient {
            space: Colorspace::LinearSrgb,
            method: InterpMethod::Linear,
            stops: alloc::vec![GradientStop { at: 0.0, c }, GradientStop { at: 1.0, c },],
        }
    }
}
