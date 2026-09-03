//! Per-uniform cache of a map input's sentinel element.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::Revision;
use lps_shared::LpsValueF32;

/// The element a map uniform's **unused** array slots carry: the declared
/// value shape's default with the mapping's key field set to the empty key,
/// already converted into the shader ABI shape.
///
/// Building it costs a shape-registry lookup, an owned `LpType` clone
/// (`to_owned_type`), a default `LpValue` build, and one model→ABI
/// conversion — and none of that depends on the frame. Before this cache all
/// of it ran on every frame, for every map uniform, before a single resolved
/// entry was looked at. The shader node runtimes keep one cache per node and
/// hand it to `materialize_shader_input`.
///
/// # Invalidation
///
/// A cached element is used only when everything it was derived from still
/// matches: the uniform's declared value-shape ref, the mapping's key field,
/// empty key and length, and the shape registry's revision (bumped by every
/// register / replace / unregister — the same conservative key slot
/// accessors use). An authored-def sync that changes any of them therefore
/// rebuilds on the next frame with no invalidate hook to forget to call, and
/// a stale element cannot outlive a shape edit.
pub struct MapInputTemplates {
    entries: Vec<MapInputTemplate>,
}

impl MapInputTemplates {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// The sentinel element cached for `uniform`, built by `build` when
    /// nothing valid is cached for `key`.
    ///
    /// `None` — from the cache or from `build` — means the mapping emits no
    /// element at all (a zero-length mapping), which is cached like any
    /// other answer so the miss path does not rerun every frame.
    pub fn empty_element<E>(
        &mut self,
        uniform: &str,
        key: MapInputTemplateKey<'_>,
        build: impl FnOnce() -> Result<Option<LpsValueF32>, E>,
    ) -> Result<Option<&LpsValueF32>, E> {
        let index = match self
            .entries
            .iter()
            .position(|entry| entry.uniform == uniform && entry.matches(&key))
        {
            Some(index) => index,
            None => {
                let element = build()?;
                // One entry per uniform: a rebuilt element replaces the
                // stale one rather than growing the cache each edit.
                self.entries.retain(|entry| entry.uniform != uniform);
                self.entries.push(MapInputTemplate {
                    uniform: String::from(uniform),
                    value_ref: String::from(key.value_ref),
                    key_field: String::from(key.key_field),
                    empty_key: key.empty_key,
                    len: key.len,
                    shapes_revision: key.shapes_revision,
                    element,
                });
                self.entries.len() - 1
            }
        };
        Ok(self.entries[index].element.as_ref())
    }
}

/// Everything a cached sentinel element was derived from, borrowed for the
/// per-frame validity check so a cache hit allocates nothing.
#[derive(Clone, Copy)]
pub struct MapInputTemplateKey<'a> {
    pub value_ref: &'a str,
    pub key_field: &'a str,
    pub empty_key: u32,
    pub len: usize,
    pub shapes_revision: Revision,
}

struct MapInputTemplate {
    uniform: String,
    value_ref: String,
    key_field: String,
    empty_key: u32,
    len: usize,
    shapes_revision: Revision,
    element: Option<LpsValueF32>,
}

impl MapInputTemplate {
    fn matches(&self, key: &MapInputTemplateKey<'_>) -> bool {
        self.value_ref == key.value_ref
            && self.key_field == key.key_field
            && self.empty_key == key.empty_key
            && self.len == key.len
            && self.shapes_revision == key.shapes_revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_lookup_with_the_same_key_reuses_the_built_element() {
        let mut templates = MapInputTemplates::new();
        let key = key_for(0, Revision::new(1));

        let first = tag(templates
            .empty_element::<()>("emitters", key, || Ok(Some(LpsValueF32::U32(1))))
            .expect("build"));
        let second = tag(templates
            .empty_element::<()>("emitters", key, || panic!("rebuilt on a hit"))
            .expect("hit"));

        assert_eq!(first, Some(1));
        assert_eq!(second, Some(1));
    }

    /// The invalidation that matters: a shape edit bumps the registry
    /// revision, and a cached element must not survive it. Caching failures
    /// are silent (memory `resolver-persistent-resolution`), so this is the
    /// test, not a comment.
    #[test]
    fn a_new_shapes_revision_rebuilds_the_element() {
        let mut templates = MapInputTemplates::new();

        templates
            .empty_element::<()>("emitters", key_for(0, Revision::new(1)), || {
                Ok(Some(LpsValueF32::U32(1)))
            })
            .expect("build");
        let rebuilt = tag(templates
            .empty_element::<()>("emitters", key_for(0, Revision::new(2)), || {
                Ok(Some(LpsValueF32::U32(2)))
            })
            .expect("rebuild"));

        assert_eq!(rebuilt, Some(2));
    }

    /// A re-authored empty key changes what the unused slots must carry.
    #[test]
    fn a_changed_empty_key_rebuilds_the_element() {
        let mut templates = MapInputTemplates::new();

        templates
            .empty_element::<()>("emitters", key_for(0, Revision::new(1)), || {
                Ok(Some(LpsValueF32::U32(1)))
            })
            .expect("build");
        let rebuilt = tag(templates
            .empty_element::<()>("emitters", key_for(9, Revision::new(1)), || {
                Ok(Some(LpsValueF32::U32(2)))
            })
            .expect("rebuild"));

        assert_eq!(rebuilt, Some(2));
    }

    /// Rebuilding replaces the uniform's entry instead of stacking a new one
    /// beside it — otherwise a live-edited shader grows this cache forever.
    #[test]
    fn rebuilding_replaces_the_uniforms_entry() {
        let mut templates = MapInputTemplates::new();

        for revision in 1..=5 {
            templates
                .empty_element::<()>("emitters", key_for(0, Revision::new(revision)), || {
                    Ok(Some(LpsValueF32::U32(1)))
                })
                .expect("build");
        }

        assert_eq!(templates.entries.len(), 1);
    }

    /// `LpsValueF32` carries no `PartialEq`, so the tests tag each element
    /// with a `U32` they can compare.
    fn tag(element: Option<&LpsValueF32>) -> Option<u32> {
        match element? {
            LpsValueF32::U32(value) => Some(*value),
            other => panic!("unexpected element {other:?}"),
        }
    }

    fn key_for(empty_key: u32, shapes_revision: Revision) -> MapInputTemplateKey<'static> {
        MapInputTemplateKey {
            value_ref: "lp::fluid::Emitter",
            key_field: "id",
            empty_key,
            len: 4,
            shapes_revision,
        }
    }
}
