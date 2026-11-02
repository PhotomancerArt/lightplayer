//! How many heap bytes a resolved payload holds — the measure the resolver
//! cache's size cap is applied to (see [`crate::dataflow::resolver::ResolverCache::set_payload_cap`]).
//!
//! An estimate, not an allocator count: it adds each owned buffer's capacity
//! times its element size, recursively, and ignores allocator headers. It is
//! bounded — the walk stops as soon as the running total passes `limit` — so
//! asking "is this over 1 KiB?" of a 6 KB palette costs a few hundred
//! bytes' worth of walk, not the whole tree.

use core::mem::size_of;

use lpc_model::{LpValue, SlotData, SlotMapKey};

/// The heap bytes `data` owns, or `None` once the count passes `limit`.
///
/// Counts the `Rc` box a [`crate::dataflow::resolver::Production`] holds
/// `data` in, too, since that is what a cached production keeps alive.
pub fn payload_bytes_within(data: &SlotData, limit: usize) -> Option<usize> {
    let mut walk = Walk { total: 0, limit };
    walk.add(size_of::<SlotData>() + 2 * size_of::<usize>())?;
    walk.slot_data(data)?;
    Some(walk.total)
}

struct Walk {
    total: usize,
    limit: usize,
}

impl Walk {
    fn add(&mut self, bytes: usize) -> Option<()> {
        self.total = self.total.saturating_add(bytes);
        (self.total <= self.limit).then_some(())
    }

    fn slot_data(&mut self, data: &SlotData) -> Option<()> {
        match data {
            SlotData::Unit { .. } => Some(()),
            SlotData::Value(value) => self.lp_value(value.value()),
            SlotData::Record(record) => {
                self.add(record.fields.capacity() * size_of::<SlotData>())?;
                for field in &record.fields {
                    self.slot_data(field)?;
                }
                Some(())
            }
            SlotData::Map(map) => {
                self.add(map.entries.len() * size_of::<(SlotMapKey, SlotData)>())?;
                for (key, value) in map.entries.iter() {
                    if let SlotMapKey::String(s) = key {
                        self.add(s.capacity())?;
                    }
                    self.slot_data(value)?;
                }
                Some(())
            }
            SlotData::Enum(e) => {
                self.add(e.variant.as_str().len() + size_of::<SlotData>())?;
                self.slot_data(&e.data)
            }
            SlotData::Option(option) => match &option.data {
                Some(inner) => {
                    self.add(size_of::<SlotData>())?;
                    self.slot_data(inner)
                }
                None => Some(()),
            },
        }
    }

    fn lp_value(&mut self, value: &LpValue) -> Option<()> {
        match value {
            LpValue::String(s) => self.add(s.capacity()),
            LpValue::Array(items) => {
                self.add(items.capacity() * size_of::<LpValue>())?;
                for item in items {
                    self.lp_value(item)?;
                }
                Some(())
            }
            LpValue::Buffer(buffer) => self.add(buffer.byte_len()),
            LpValue::Struct { name, fields } => {
                self.add(name.as_ref().map_or(0, |n| n.capacity()))?;
                self.add(fields.capacity() * size_of::<(alloc::string::String, LpValue)>())?;
                for (field, item) in fields {
                    self.add(field.capacity())?;
                    self.lp_value(item)?;
                }
                Some(())
            }
            LpValue::Enum {
                payload: Some(payload),
                ..
            } => {
                self.add(size_of::<LpValue>())?;
                self.lp_value(payload)
            }
            _ => Some(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use lpc_model::{Revision, SlotRecord, WithRevision};

    #[test]
    fn a_scalar_is_its_box_only() {
        let data = SlotData::Value(WithRevision::new(Revision::new(1), LpValue::F32(1.0)));
        let bytes = payload_bytes_within(&data, usize::MAX).expect("unbounded");
        assert_eq!(bytes, size_of::<SlotData>() + 2 * size_of::<usize>());
    }

    #[test]
    fn an_array_counts_its_elements_and_stops_at_the_limit() {
        let items = vec![LpValue::String(String::from("abcdefgh")); 64];
        let data = SlotData::Value(WithRevision::new(Revision::new(1), LpValue::Array(items)));
        let bytes = payload_bytes_within(&data, usize::MAX).expect("unbounded");
        assert!(bytes >= 64 * (size_of::<LpValue>() + 8), "{bytes}");
        assert_eq!(payload_bytes_within(&data, 256), None);
    }

    #[test]
    fn a_record_counts_its_fields() {
        let leaf = || SlotData::Value(WithRevision::new(Revision::new(1), LpValue::F32(0.0)));
        let data = SlotData::Record(SlotRecord::new(vec![leaf(), leaf(), leaf()]));
        let bytes = payload_bytes_within(&data, usize::MAX).expect("unbounded");
        assert!(bytes >= 4 * size_of::<SlotData>(), "{bytes}");
    }
}
