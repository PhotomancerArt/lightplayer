use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use lpc_model::{ChannelName, FrameId};

use super::BindingError;
use super::BindingId;
use super::binding_entry::{BindingDraft, BindingEntry, BindingTarget, channels_touched};

/// Owns binding identity, metadata, and indexes for bus channel validation.
/// Does not store resolved runtime values.
pub struct BindingRegistry {
    next_id: u32,
    entries: BTreeMap<BindingId, BindingEntry>,
    /// Bindings that reference a channel via source or target [`BindingSource::BusChannel`] / [`BindingTarget::BusChannel`].
    channel_refs: BTreeMap<ChannelName, Vec<BindingId>>,
    /// Bindings whose target is [`BindingTarget::BusChannel`], by channel.
    bus_targets: BTreeMap<ChannelName, Vec<BindingId>>,
}

impl Default for BindingRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BindingRegistry {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            entries: BTreeMap::new(),
            channel_refs: BTreeMap::new(),
            bus_targets: BTreeMap::new(),
        }
    }

    /// Registers a binding; assigns a new non-zero [`BindingId`] and sets [`BindingEntry::version`] to `frame`.
    pub fn register(
        &mut self,
        draft: BindingDraft,
        frame: FrameId,
    ) -> Result<BindingId, BindingError> {
        let channels = channels_touched(&draft.source, &draft.target);

        for ch in &channels {
            if let Some(ids) = self.channel_refs.get(ch) {
                for &existing_id in ids {
                    if let Some(existing) = self.entries.get(&existing_id)
                        && existing.kind != draft.kind
                    {
                        return Err(BindingError::KindMismatch {
                            channel: ch.clone(),
                            established: existing.kind,
                            attempted: draft.kind,
                        });
                    }
                }
            }
        }

        if let BindingTarget::BusChannel(ref ch) = draft.target {
            if let Some(ids) = self.bus_targets.get(ch) {
                for &existing_id in ids {
                    if let Some(existing) = self.entries.get(&existing_id)
                        && existing.priority == draft.priority
                    {
                        return Err(BindingError::DuplicateProviderPriority {
                            channel: ch.clone(),
                            priority: draft.priority,
                        });
                    }
                }
            }
        }

        let id = self.allocate_id()?;

        let entry = BindingEntry {
            id,
            source: draft.source,
            target: draft.target,
            priority: draft.priority,
            kind: draft.kind,
            version: frame,
            owner: draft.owner,
        };

        let ch_for_targets = channels_touched(&entry.source, &entry.target);
        if let BindingTarget::BusChannel(ref ch) = entry.target {
            self.bus_targets.entry(ch.clone()).or_default().push(id);
        }

        for ch in ch_for_targets {
            self.channel_refs.entry(ch).or_default().push(id);
        }

        self.entries.insert(id, entry);
        Ok(id)
    }

    pub fn unregister(
        &mut self,
        id: BindingId,
        _frame: FrameId,
    ) -> Result<BindingEntry, BindingError> {
        let entry = self
            .entries
            .remove(&id)
            .ok_or(BindingError::UnknownBinding { id })?;

        let ch_list = channels_touched(&entry.source, &entry.target);

        if let BindingTarget::BusChannel(ref ch) = entry.target {
            remove_from_vec(self.bus_targets.get_mut(ch), id);
        }

        for ch in ch_list {
            remove_from_vec(self.channel_refs.get_mut(&ch), id);
        }

        Ok(entry)
    }

    pub fn get(&self, id: BindingId) -> Option<&BindingEntry> {
        self.entries.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &BindingEntry> {
        self.entries.values()
    }

    /// Bindings whose **target** is [`BindingTarget::BusChannel`]`(channel)` (providers for that channel).
    pub fn providers_for_bus<'a>(
        &'a self,
        channel: &'a ChannelName,
    ) -> impl Iterator<Item = &'a BindingEntry> + 'a {
        self.bus_targets
            .get(channel)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(move |bid| self.entries.get(bid))
    }

    fn allocate_id(&mut self) -> Result<BindingId, BindingError> {
        let start = self.next_id;
        loop {
            let id = BindingId::new(self.next_id);
            self.next_id = self.next_id.wrapping_add(1);
            if self.next_id == 0 {
                self.next_id = 1;
            }

            if !self.entries.contains_key(&id) {
                return Ok(id);
            }

            if self.next_id == start {
                return Err(BindingError::IdExhausted);
            }
        }
    }
}

fn remove_from_vec(ids: Option<&mut Vec<BindingId>>, id: BindingId) {
    if let Some(v) = ids {
        v.retain(|x| *x != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BindingDraft;
    use crate::BindingPriority;
    use crate::BindingSource;
    use crate::BindingTarget;
    use alloc::string::String;
    use alloc::vec;
    use lpc_model::prop::prop_path::parse_path;
    use lpc_model::{ChannelName, Kind, NodeId};
    use lpc_source::SrcValueSpec;

    fn ch(s: &str) -> ChannelName {
        ChannelName(String::from(s))
    }

    fn path(s: &str) -> lpc_model::PropPath {
        parse_path(s).expect("test path")
    }

    #[test]
    fn register_assigns_stable_nonzero_binding_id() {
        let mut reg = BindingRegistry::new();
        let frame = FrameId::new(1);
        let id = reg
            .register(
                BindingDraft {
                    source: BindingSource::Literal(SrcValueSpec::Literal(
                        lpc_model::ModelValue::F32(1.0),
                    )),
                    target: BindingTarget::BusChannel(ch("out/a")),
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: NodeId::new(1),
                },
                frame,
            )
            .expect("register");
        assert_ne!(id.as_u32(), 0);
        let again = reg
            .register(
                BindingDraft {
                    source: BindingSource::BusChannel(ch("out/a")),
                    target: BindingTarget::NodeInput {
                        node: NodeId::new(2),
                        input: path("in"),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Amplitude,
                    owner: NodeId::new(2),
                },
                frame,
            )
            .expect("register");
        assert_ne!(id, again);
        assert_eq!(reg.get(id).expect("entry").id, id);
    }

    #[test]
    fn unregister_removes_binding_and_updates_indexes() {
        let mut reg = BindingRegistry::new();
        let frame = FrameId::new(3);
        let id = reg
            .register(
                BindingDraft {
                    source: BindingSource::Literal(SrcValueSpec::Literal(
                        lpc_model::ModelValue::F32(0.0),
                    )),
                    target: BindingTarget::BusChannel(ch("bus/z")),
                    priority: BindingPriority::new(10),
                    kind: Kind::Ratio,
                    owner: NodeId::new(0),
                },
                frame,
            )
            .expect("register");
        assert_eq!(reg.providers_for_bus(&ch("bus/z")).count(), 1);
        let removed = reg.unregister(id, frame).expect("unregister");
        assert_eq!(removed.id, id);
        assert!(reg.get(id).is_none());
        assert_eq!(reg.providers_for_bus(&ch("bus/z")).count(), 0);
    }

    #[test]
    fn providers_for_bus_returns_bus_target_entries() {
        let mut reg = BindingRegistry::new();
        let frame = FrameId::new(1);
        let c = ch("video/out");
        reg.register(
            BindingDraft {
                source: BindingSource::NodeOutput {
                    node: NodeId::new(1),
                    output: path("color"),
                },
                target: BindingTarget::BusChannel(c.clone()),
                priority: BindingPriority::new(5),
                kind: Kind::Color,
                owner: NodeId::new(1),
            },
            frame,
        )
        .expect("register");
        let mut providers: Vec<_> = reg.providers_for_bus(&c).map(|e| e.owner).collect();
        providers.sort_by_key(|n| n.as_u32());
        assert_eq!(providers, vec![NodeId::new(1)]);
    }

    #[test]
    fn kind_mismatch_on_same_bus_channel_errors() {
        let mut reg = BindingRegistry::new();
        let frame = FrameId::new(1);
        let c = ch("shared");
        reg.register(
            BindingDraft {
                source: BindingSource::Literal(SrcValueSpec::Literal(lpc_model::ModelValue::F32(
                    1.0,
                ))),
                target: BindingTarget::BusChannel(c.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Amplitude,
                owner: NodeId::new(1),
            },
            frame,
        )
        .expect("first");
        let err = reg
            .register(
                BindingDraft {
                    source: BindingSource::BusChannel(c.clone()),
                    target: BindingTarget::NodeInput {
                        node: NodeId::new(2),
                        input: path("x"),
                    },
                    priority: BindingPriority::new(0),
                    kind: Kind::Ratio,
                    owner: NodeId::new(2),
                },
                frame,
            )
            .expect_err("kind mismatch");
        assert!(matches!(err, BindingError::KindMismatch { .. }));
    }

    #[test]
    fn equal_priority_providers_on_same_bus_channel_errors() {
        let mut reg = BindingRegistry::new();
        let frame = FrameId::new(1);
        let c = ch("x");
        reg.register(
            BindingDraft {
                source: BindingSource::Literal(SrcValueSpec::Literal(lpc_model::ModelValue::F32(
                    1.0,
                ))),
                target: BindingTarget::BusChannel(c.clone()),
                priority: BindingPriority::new(7),
                kind: Kind::Phase,
                owner: NodeId::new(1),
            },
            frame,
        )
        .expect("first");
        let err = reg
            .register(
                BindingDraft {
                    source: BindingSource::Literal(SrcValueSpec::Literal(
                        lpc_model::ModelValue::F32(2.0),
                    )),
                    target: BindingTarget::BusChannel(c.clone()),
                    priority: BindingPriority::new(7),
                    kind: Kind::Phase,
                    owner: NodeId::new(2),
                },
                frame,
            )
            .expect_err("dup priority");
        assert!(matches!(
            err,
            BindingError::DuplicateProviderPriority { .. }
        ));
    }

    #[test]
    fn binding_version_follows_frame() {
        let mut reg = BindingRegistry::new();
        let f10 = FrameId::new(10);
        let id = reg
            .register(
                BindingDraft {
                    source: BindingSource::Literal(SrcValueSpec::Literal(
                        lpc_model::ModelValue::Bool(true),
                    )),
                    target: BindingTarget::BusChannel(ch("b")),
                    priority: BindingPriority::new(0),
                    kind: Kind::Bool,
                    owner: NodeId::new(0),
                },
                f10,
            )
            .expect("register");
        assert_eq!(reg.get(id).expect("entry").version, f10);
        let f11 = FrameId::new(11);
        let id2 = reg
            .register(
                BindingDraft {
                    source: BindingSource::Literal(SrcValueSpec::Literal(
                        lpc_model::ModelValue::Bool(false),
                    )),
                    target: BindingTarget::BusChannel(ch("b2")),
                    priority: BindingPriority::new(0),
                    kind: Kind::Bool,
                    owner: NodeId::new(0),
                },
                f11,
            )
            .expect("register");
        assert_eq!(reg.get(id2).expect("entry").version, f11);
    }
}
