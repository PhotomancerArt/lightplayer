//! Per-uniform cache of the resolver keys the authored-def sync reads.

use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use lp_collection::VecMap;
use lpc_model::SlotPath;

use crate::dataflow::resolver::QueryKey;
use crate::node::{NodeError, TickContext};

/// Pre-built [`QueryKey`]s for the authored fields the per-tick def sync
/// reads, one set per consumed uniform.
///
/// Every tick, both shader node runtimes re-read ~16 authored fields per
/// uniform so that a live authoring edit lands on the next frame
/// (`sync_shader_slot_def_from_authored`). Each read used to `format!` the
/// path (`consumed[<key>].mapping.some.len`), `SlotPath::parse` it into a
/// `Vec` of heap `String` segments, build a key, hash it, and drop the whole
/// thing again — for an answer that changes only when the author edits the
/// project. On Meteor that was the largest single source of per-frame
/// allocations. This holds the key instead.
///
/// # What is cached, and what is not
///
/// Keys, never [`crate::dataflow::resolver::query_intern::QueryId`]s. A
/// `QueryKey` is valid forever; an id is valid only within one structural
/// epoch, so caching ids would need an invalidation hook that caching keys
/// does not.
///
/// Keys are built **lazily, one field at a time**, on the first tick that
/// actually reads that field. That is not just laziness: probing an authored
/// path the sync deliberately skips (`.key.some` on a value slot,
/// `.phasor.some` on a non-phasor) persists a resolver route entry for the
/// life of the project (ADR 2026-07-31), which is exactly what the gates in
/// `sync_shader_slot_def_from_authored` exist to avoid. Building the key
/// eagerly would not resolve anything by itself, but it would put a second
/// copy of every unread path in RAM for nothing.
///
/// # Invalidation
///
/// - A uniform **added** to the authored def gets its set on the tick that
///   first syncs it ([`Self::uniform`] creates on miss).
/// - A uniform **removed** loses its set: the sync loop calls
///   [`Self::retain_uniforms`] with the live key set, so this map's length
///   tracks the uniform count instead of growing across live edits.
/// - A **structural epoch** change clears everything ([`Self::uniform`]
///   compares the epoch it last saw). Not for correctness — the keys stay
///   valid — but because the shared `Rc` came from the intern table, and the
///   table drops its half on `invalidate_structure`. Re-sharing after the
///   epoch keeps one copy of each path in RAM rather than two.
pub struct AuthoredFieldKeys {
    per_uniform: VecMap<String, UniformFieldKeys>,
    /// The structural epoch the cached keys were interned in.
    epoch: u64,
}

/// One consumed uniform's authored-field keys, indexed by [`AuthoredField`].
pub struct UniformFieldKeys {
    keys: [Option<Rc<QueryKey>>; AuthoredField::COUNT],
}

/// An authored field of one shader uniform, as a fixed index into
/// [`UniformFieldKeys`].
///
/// The order is arbitrary; only the discriminants' distinctness matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoredField {
    /// The uniform's own consumed slot — the value read, not a def field.
    /// Its path is the bare uniform name, not `consumed[<name>]`.
    Own,
    Kind,
    Value,
    KeySome,
    LenSome,
    PhasorSome,
    GradientSome,
    DefaultSome,
    MinSome,
    MaxSome,
    MappingKind,
    MappingLen,
    MappingKey,
    MappingEmptyKey,
    Label,
    Description,
}

impl AuthoredField {
    /// How many fields there are — the width of a [`UniformFieldKeys`] row.
    pub const COUNT: usize = 16;

    /// The suffix this field adds to `consumed[<uniform>]`, or `None` for
    /// [`Self::Own`], whose path is the uniform name alone.
    fn suffix(self) -> Option<&'static str> {
        Some(match self {
            Self::Own => return None,
            Self::Kind => "kind",
            Self::Value => "value",
            Self::KeySome => "key.some",
            Self::LenSome => "len.some",
            Self::PhasorSome => "phasor.some",
            Self::GradientSome => "gradient.some",
            Self::DefaultSome => "default.some",
            Self::MinSome => "min.some",
            Self::MaxSome => "max.some",
            Self::MappingKind => "mapping.some.kind",
            Self::MappingLen => "mapping.some.len",
            Self::MappingKey => "mapping.some.key",
            Self::MappingEmptyKey => "mapping.some.empty_key",
            Self::Label => "label",
            Self::Description => "description",
        })
    }

    /// The authored path this field names on `uniform`.
    pub(super) fn path_for(self, uniform: &str) -> String {
        match self.suffix() {
            Some(suffix) => format!("consumed[{uniform}].{suffix}"),
            None => String::from(uniform),
        }
    }
}

impl AuthoredFieldKeys {
    pub fn new() -> Self {
        Self {
            per_uniform: VecMap::new(),
            epoch: 0,
        }
    }

    /// The key set for `uniform`, empty on first sight.
    ///
    /// `epoch` is the resolver's current structural epoch
    /// ([`TickContext::structure_epoch`]); a change drops every cached key so
    /// the rebuilt ones re-share the intern table's copy.
    pub fn uniform(&mut self, uniform: &str, epoch: u64) -> &mut UniformFieldKeys {
        if self.epoch != epoch {
            self.epoch = epoch;
            self.per_uniform.clear();
        }
        if self.per_uniform.get(uniform).is_none() {
            self.per_uniform
                .insert(String::from(uniform), UniformFieldKeys::new());
        }
        self.per_uniform
            .get_mut(uniform)
            .expect("the entry was just inserted")
    }

    /// Drop the key sets of uniforms `keep` no longer recognises.
    ///
    /// Called once per sync pass with the runtime's live uniform names: a
    /// uniform an overlay `Remove` took away must not leave its keys behind,
    /// or a live-edited shader grows this cache forever.
    pub fn retain_uniforms(&mut self, keep: impl Fn(&str) -> bool) {
        self.per_uniform.retain(|name, _| keep(name.as_str()));
    }

    /// How many uniforms have a key set. Tracks the live uniform count.
    pub fn len(&self) -> usize {
        self.per_uniform.len()
    }

    /// How many of `uniform`'s fields have a key built, or `None` when the
    /// uniform has no set. Test-facing: the laziness above is what keeps
    /// authored paths the sync deliberately skips out of the resolver.
    #[cfg(test)]
    pub fn built_for(&self, uniform: &str) -> Option<usize> {
        self.per_uniform.get(uniform).map(UniformFieldKeys::built)
    }
}

impl Default for AuthoredFieldKeys {
    fn default() -> Self {
        Self::new()
    }
}

impl UniformFieldKeys {
    fn new() -> Self {
        Self {
            keys: core::array::from_fn(|_| None),
        }
    }

    /// The resolver key for `field` on `uniform`, built on first use.
    ///
    /// The returned borrow is of the cache, not of `ctx`, so the caller can
    /// hand it straight to [`TickContext::resolve`].
    pub fn key<'k>(
        &'k mut self,
        ctx: &mut TickContext<'_>,
        uniform: &str,
        field: AuthoredField,
    ) -> Result<&'k QueryKey, NodeError> {
        let index = field as usize;
        if self.keys[index].is_none() {
            let path = field.path_for(uniform);
            let slot = SlotPath::parse(&path).map_err(|e| {
                NodeError::msg(format!("invalid authored shader path {path:?}: {e}"))
            })?;
            let key = QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot,
            };
            self.keys[index] = Some(ctx.intern_key(&key));
        }
        Ok(self.keys[index].as_deref().expect("the key was just built"))
    }

    /// How many of this uniform's fields have a key built.
    #[cfg(test)]
    fn built(&self) -> usize {
        self.keys.iter().filter(|key| key.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_path_hangs_off_the_uniforms_consumed_entry() {
        assert_eq!(
            AuthoredField::MappingEmptyKey.path_for("meteors"),
            "consumed[meteors].mapping.some.empty_key"
        );
        assert_eq!(
            AuthoredField::Kind.path_for("reach"),
            "consumed[reach].kind"
        );
    }

    /// The uniform's own value is read at the bare name — the projection
    /// `resolve_or_default_input` uses — not under `consumed[...]`.
    #[test]
    fn the_own_field_is_the_bare_uniform_name() {
        assert_eq!(AuthoredField::Own.path_for("meteors"), "meteors");
    }

    /// Every discriminant must fit the row, or `key` indexes out of bounds.
    #[test]
    fn every_field_indexes_inside_the_row() {
        let fields = [
            AuthoredField::Own,
            AuthoredField::Kind,
            AuthoredField::Value,
            AuthoredField::KeySome,
            AuthoredField::LenSome,
            AuthoredField::PhasorSome,
            AuthoredField::GradientSome,
            AuthoredField::DefaultSome,
            AuthoredField::MinSome,
            AuthoredField::MaxSome,
            AuthoredField::MappingKind,
            AuthoredField::MappingLen,
            AuthoredField::MappingKey,
            AuthoredField::MappingEmptyKey,
            AuthoredField::Label,
            AuthoredField::Description,
        ];
        assert_eq!(fields.len(), AuthoredField::COUNT);
        for field in fields {
            assert!(
                (field as usize) < AuthoredField::COUNT,
                "{field:?} is outside the row"
            );
        }
    }

    #[test]
    fn retain_drops_the_uniforms_the_runtime_no_longer_has() {
        let mut keys = AuthoredFieldKeys::new();
        keys.uniform("a", 0);
        keys.uniform("b", 0);
        assert_eq!(keys.len(), 2);

        keys.retain_uniforms(|name| name == "a");

        assert_eq!(keys.len(), 1);
    }

    /// A structural epoch voids nothing about a key's *meaning* — it voids
    /// the sharing with the intern table, so the cache re-interns rather
    /// than keeping a private copy of every path alive.
    #[test]
    fn a_new_structural_epoch_drops_the_cached_keys() {
        let mut keys = AuthoredFieldKeys::new();
        keys.uniform("a", 0);
        keys.uniform("b", 0);

        keys.uniform("a", 1);

        assert_eq!(keys.len(), 1, "the epoch change must clear the map");
    }
}
