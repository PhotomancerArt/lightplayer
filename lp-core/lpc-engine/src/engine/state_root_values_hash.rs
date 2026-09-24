//! A node state slot root's content hash, with every revision stamp left out.
//!
//! A runtime state root's field stamps say when a node last *touched* a
//! field, not when it changed it: nodes restamp their produced fields on
//! every produce — a product handle's revision is the resolver's
//! content-changed signal, so it must move — even when the value holds. A
//! read gated on those stamps resends every alive node's state root on
//! every read. This hash covers exactly what a client's mirror would show
//! differently: the shape id, the structure (record fields, map keys, enum
//! variants, option presence) and every leaf value. It leaves out value
//! `changed_at` stamps and the container revisions, which is the point.
//!
//! One exception: a unit slot has no value — its revision IS its content
//! (a pulse), so it is hashed.
//!
//! Each leaf value is hashed through the wire serializer
//! ([`lpc_wire::ser_write_json_fnv64`]), and the leaf hashes fold into one
//! FNV-1a 64 state with a tag byte per node kind, so a record of `[1, 2]`
//! and a map with keys `1, 2` cannot collide by accident.

use lpc_model::{SlotDataAccess, SlotMapKey, SlotShapeId};
use lpc_wire::ser_write_json_fnv64;

/// Hash `data` (a state root of shape `shape`) without its revision stamps.
pub(super) fn state_root_values_hash(shape: SlotShapeId, data: SlotDataAccess<'_>) -> u64 {
    let mut hasher = ValuesHasher::new();
    hasher.word(u64::from(shape.raw()));
    hasher.data(data);
    hasher.hash
}

/// One tag per slot-data kind, and one closing the variable-length ones.
#[derive(Clone, Copy)]
enum Tag {
    Unit = 1,
    Value,
    Record,
    Map,
    Enum,
    OptionNone,
    OptionSome,
    Custom,
    End,
    KeyString,
    KeyI32,
    KeyU32,
}

/// FNV-1a 64 over tags and 64-bit words.
struct ValuesHasher {
    hash: u64,
}

impl ValuesHasher {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    const fn new() -> Self {
        Self {
            hash: Self::OFFSET_BASIS,
        }
    }

    fn data(&mut self, data: SlotDataAccess<'_>) {
        match data {
            SlotDataAccess::Unit(revision) => {
                self.tag(Tag::Unit);
                self.word(revision.as_i64() as u64);
            }
            SlotDataAccess::Value(value) => {
                self.tag(Tag::Value);
                self.word(ser_write_json_fnv64(&value.value()));
            }
            SlotDataAccess::Record(record) => {
                self.tag(Tag::Record);
                let mut index = 0;
                while let Some(field) = record.field(index) {
                    self.data(field);
                    index += 1;
                }
                self.tag(Tag::End);
            }
            SlotDataAccess::Map(map) => {
                self.tag(Tag::Map);
                for key in map.keys() {
                    self.key(&key);
                    if let Some(value) = map.get(&key) {
                        self.data(value);
                    }
                }
                self.tag(Tag::End);
            }
            SlotDataAccess::Enum(en) => {
                self.tag(Tag::Enum);
                self.word(ser_write_json_fnv64(&en.variant()));
                self.data(en.data());
            }
            SlotDataAccess::Option(option) => match option.data() {
                Some(some) => {
                    self.tag(Tag::OptionSome);
                    self.data(some);
                }
                None => self.tag(Tag::OptionNone),
            },
            SlotDataAccess::Custom(custom) => {
                self.tag(Tag::Custom);
                self.word(u64::from(custom.custom_codec_id().raw()));
                match lpc_model::slot_codec::snapshot_custom_slot_data(
                    custom.custom_codec_id(),
                    custom,
                ) {
                    Ok(inner) => self.data(inner),
                    // A codec the snapshot writer cannot open either: its
                    // own revision is all there is to go on.
                    Err(_) => self.word(custom.custom_revision().as_i64() as u64),
                }
            }
        }
    }

    fn key(&mut self, key: &SlotMapKey) {
        match key {
            SlotMapKey::String(key) => {
                self.tag(Tag::KeyString);
                self.word(ser_write_json_fnv64(key));
            }
            SlotMapKey::I32(key) => {
                self.tag(Tag::KeyI32);
                self.word(*key as u32 as u64);
            }
            SlotMapKey::U32(key) => {
                self.tag(Tag::KeyU32);
                self.word(u64::from(*key));
            }
        }
    }

    fn tag(&mut self, tag: Tag) {
        self.byte(tag as u8);
    }

    fn word(&mut self, word: u64) {
        for byte in word.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn byte(&mut self, byte: u8) {
        self.hash ^= u64::from(byte);
        self.hash = self.hash.wrapping_mul(Self::PRIME);
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use lpc_model::{LpValue, Revision, SlotData, SlotRecord, WithRevision};

    use super::*;

    #[test]
    fn a_restamped_value_hashes_the_same() {
        let before = record(vec![value(3, LpValue::F32(1.0)), value(3, LpValue::U32(7))]);
        let after = record(vec![
            value(9, LpValue::F32(1.0)),
            value(12, LpValue::U32(7)),
        ]);
        assert_eq!(hash(&before), hash(&after));
    }

    #[test]
    fn a_changed_value_changes_the_hash() {
        let before = record(vec![value(3, LpValue::F32(1.0))]);
        let after = record(vec![value(3, LpValue::F32(0.5))]);
        assert_ne!(hash(&before), hash(&after));
    }

    #[test]
    fn a_changed_shape_changes_the_hash() {
        let data = record(vec![value(3, LpValue::F32(1.0))]);
        assert_ne!(
            state_root_values_hash(SlotShapeId::new(1), data.access()),
            state_root_values_hash(SlotShapeId::new(2), data.access()),
        );
    }

    #[test]
    fn a_unit_pulse_is_content() {
        assert_ne!(
            hash(&SlotData::Unit {
                revision: Revision::new(3)
            }),
            hash(&SlotData::Unit {
                revision: Revision::new(4)
            }),
        );
    }

    #[test]
    fn nesting_is_part_of_the_hash() {
        let flat = record(vec![value(1, LpValue::U32(1)), value(1, LpValue::U32(2))]);
        let nested = record(vec![
            record(vec![value(1, LpValue::U32(1))]),
            value(1, LpValue::U32(2)),
        ]);
        assert_ne!(hash(&flat), hash(&nested));
    }

    fn hash(data: &SlotData) -> u64 {
        state_root_values_hash(SlotShapeId::new(1), data.access())
    }

    fn value(changed_at: i64, value: LpValue) -> SlotData {
        SlotData::Value(WithRevision::new(Revision::new(changed_at), value))
    }

    fn record(fields: alloc::vec::Vec<SlotData>) -> SlotData {
        SlotData::Record(SlotRecord::with_revision(Revision::new(1), fields))
    }
}
