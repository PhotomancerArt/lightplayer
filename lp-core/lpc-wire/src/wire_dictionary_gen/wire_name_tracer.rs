//! Every name the wire types can put on the wire, found by tracing their
//! `Deserialize` impls (the technique `serde-reflection` uses, hand-rolled:
//! we only need names, not formats).
//!
//! [`trace_wire_names`] deserializes `WireServerMessage` and `ClientMessage`
//! from a [`Deserializer`] that invents every value. The derived impls tell
//! it what they are as they ask:
//! - `deserialize_struct` hands over the struct's field names, after
//!   `rename`/`rename_all`, `skip`ped fields already gone;
//! - `deserialize_enum` hands over the variant names, and the variant access
//!   the derived visitor then calls says what shape the chosen one is: a unit
//!   variant is a JSON string (a **value**, or a **key** when the enum is a map
//!   key), any other is the key of a one-entry object (a **key**).
//!
//! Serde attributes the wire uses (`rename`, `rename_all`, `default`,
//! `skip_serializing_if`, `with`, `transparent`) need nothing: the derived impl
//! applies them before it asks. `tag`/`untagged`/`flatten` are linted out.
//! Types that deserialize through their own code (`serde_base64`, `RawValue`,
//! string-parsed ids) are leaves: whatever they ask for is invented, and if
//! the invented value does not parse, that subtree fails.
//!
//! **Failures are fine.** A failed subtree fails its parent, but every name
//! was recorded on the way in, so a run only needs to *reach* things. To reach
//! all of them, runs repeat from the roots until nothing is left unexplored:
//! - each enum picks a variant it has not tried yet;
//! - each struct presents its fields starting from one it has not reached,
//!   so a field that always fails stops hiding the ones after it;
//! - otherwise both steer toward choices that lead, by what earlier runs
//!   saw, to a type that still has something unexplored ([`TraceState`]);
//! - recursion is cut by a per-type limit on the stack, and options,
//!   sequences and maps go empty past a soft depth.
//!
//! Everything is deterministic, so the output is too.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::string::{String, ToString};
use std::vec::Vec;

use serde::Deserialize;
use serde::de::value::StrDeserializer;
use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor,
};

use crate::{ClientMessage, WireServerMessage};

/// The names reachable from the wire's two root types.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WireNames {
    /// Struct field names and data-carrying variant names (and unit variants
    /// that are map keys): what a JSON object uses as keys.
    pub keys: BTreeSet<&'static str>,
    /// Unit variant names: JSON string values.
    pub values: BTreeSet<&'static str>,
    /// Distinct structs (and struct variants) reached.
    pub structs: usize,
    /// Distinct enums reached.
    pub enums: usize,
    /// Types left with an untried variant or an unreached field, with how
    /// many of their choices were taken (empty when the trace saw everything).
    pub unreached: Vec<String>,
    /// Runs from the roots until the trace settled.
    pub runs: usize,
}

/// Trace every name `WireServerMessage` and `ClientMessage` can carry.
pub fn trace_wire_names() -> WireNames {
    let mut st = TraceState::default();
    let (mut quiet, mut runs) = (0, 0);
    loop {
        st.recompute_hot();
        if runs > 0 && st.hot.is_empty() {
            break;
        }
        assert!(runs < MAX_RUNS, "the wire name trace did not settle");
        let before = st.discoveries;
        // Errors are expected (see the module docs); the names are recorded.
        let _ = WireServerMessage::deserialize(Tracer::root(&mut st));
        let _ = ClientMessage::deserialize(Tracer::root(&mut st));
        debug_assert!(st.stack.is_empty());
        runs += 1;
        quiet = if st.discoveries == before {
            quiet + 1
        } else {
            0
        };
        if quiet >= QUIET_RUNS {
            // What is left cannot be reached (a field behind one that
            // always fails in a tuple, say); it is reported, not hidden.
            break;
        }
    }
    let unreached = st
        .frontier()
        .map(|ty| {
            let t = st.enums.get(&ty).or(st.structs.get(&ty));
            let (taken, count) = t.map_or((0, 0), |t| (t.taken.len(), t.count));
            std::format!("{} ({taken} of {count})", ty.0)
        })
        .collect();
    WireNames {
        keys: st.keys,
        values: st.values,
        structs: st.structs.len(),
        enums: st.enums.len(),
        unreached,
        runs,
    }
}

/// Runs in a row that may discover nothing while unexplored choices remain,
/// before the trace gives up on them.
const QUIET_RUNS: usize = 256;
/// A trace that has not settled by now is a bug in the tracer.
const MAX_RUNS: usize = 20_000;
/// Past this depth, options are `None` and sequences and maps are empty.
const SOFT_DEPTH: usize = 24;
/// Past this depth, everything fails.
const HARD_DEPTH: usize = 64;
/// A named type may be on the stack this many times (recursion is cut there).
const MAX_SAME_ON_STACK: usize = 2;

/// serde_json's `RawValue` newtype name: its `Deserialize` wants a one-entry
/// map of this key to the raw text.
const RAW_VALUE_TOKEN: &str = "$serde_json::private::RawValue";

/// A named type: its name and its field or variant list's address (two types
/// may share a name).
type TypeKey = (&'static str, usize);

/// Which variant (enums) or field (structs) of a type a child hangs under.
type Choice = (TypeKey, usize);

/// The trace's memory across runs.
///
/// Guidance: every recorded edge says "choosing this variant or field of
/// this type leads to that type". A type with an untried variant or an
/// unreached field is on the **frontier**; a type is **hot** when it is on
/// the frontier or a choice of it leads to a hot type. Each enum and struct
/// steers into hot choices, so a run walks toward whatever is still
/// unexplored however deep it sits, and the trace ends when nothing is hot.
#[derive(Default)]
struct TraceState {
    keys: BTreeSet<&'static str>,
    values: BTreeSet<&'static str>,
    enums: BTreeMap<TypeKey, ChoiceTrace>,
    structs: BTreeMap<TypeKey, ChoiceTrace>,
    edges: BTreeMap<Choice, BTreeSet<TypeKey>>,
    hot: BTreeSet<TypeKey>,
    /// The named types being deserialized, with the choice taken in each.
    stack: Vec<Choice>,
    /// Bumped by everything new: a name, a type, an edge, a variant tried, a
    /// field reached.
    discoveries: u64,
}

/// One enum's variants or one struct's fields.
#[derive(Default)]
struct ChoiceTrace {
    count: usize,
    taken: BTreeSet<usize>,
    round_robin: usize,
}

impl ChoiceTrace {
    fn untaken(&self) -> Option<usize> {
        (0..self.count).find(|i| !self.taken.contains(i))
    }
}

impl TraceState {
    fn found(&mut self, new: bool) {
        if new {
            self.discoveries += 1;
        }
    }

    fn key(&mut self, name: &'static str) {
        let new = self.keys.insert(name);
        self.found(new);
    }

    fn value(&mut self, name: &'static str) {
        let new = self.values.insert(name);
        self.found(new);
    }

    /// Push `ty` with its first `choice`, recording the edge from the
    /// choice it hangs under.
    fn enter(&mut self, ty: TypeKey, choice: usize) -> Result<(), TraceError> {
        let on_stack = self.stack.iter().filter(|(t, _)| *t == ty).count();
        if self.stack.len() >= HARD_DEPTH || on_stack >= MAX_SAME_ON_STACK {
            return Err(TraceError::new("recursion cut"));
        }
        if let Some(&parent) = self.stack.last() {
            let new = self.edges.entry(parent).or_default().insert(ty);
            self.found(new);
        }
        self.stack.push((ty, choice));
        Ok(())
    }

    fn leave(&mut self) {
        self.stack.pop();
    }

    /// The struct on top of the stack moved on to `field`.
    fn at_field(&mut self, field: usize) {
        if let Some(top) = self.stack.last_mut() {
            let ty = top.0;
            top.1 = field;
            let new = self.structs.entry(ty).or_default().taken.insert(field);
            self.found(new);
        }
    }

    /// Record a struct's fields and choose where its field order starts: an
    /// unreached field, else one leading somewhere hot, else the next in turn.
    fn struct_start(&mut self, ty: TypeKey, fields: &'static [&'static str]) -> usize {
        for &f in fields {
            self.key(f);
        }
        self.choose(ty, fields.len(), false)
    }

    /// Choose a variant: an untried one, else one leading somewhere hot, else
    /// the next in turn.
    fn pick_variant(&mut self, ty: TypeKey, count: usize) -> usize {
        let pick = self.choose(ty, count, true);
        let new = self.enums.entry(ty).or_default().taken.insert(pick);
        self.found(new);
        pick
    }

    fn choose(&mut self, ty: TypeKey, count: usize, is_enum: bool) -> usize {
        let map = if is_enum { &self.enums } else { &self.structs };
        let new = !map.contains_key(&ty);
        self.found(new);
        let hot: Vec<usize> = (0..count).filter(|&c| self.leads_hot((ty, c))).collect();
        let map = if is_enum {
            &mut self.enums
        } else {
            &mut self.structs
        };
        let t = map.entry(ty).or_default();
        t.count = count;
        if let Some(i) = t.untaken() {
            return i;
        }
        let pick = if hot.is_empty() {
            t.round_robin % count
        } else {
            hot[t.round_robin % hot.len()]
        };
        t.round_robin += 1;
        pick
    }

    /// Whether `choice` leads to a hot type that is not already being
    /// deserialized (going round a cycle is not progress).
    fn leads_hot(&self, choice: Choice) -> bool {
        self.edges.get(&choice).is_some_and(|to| {
            to.iter().any(|t| {
                self.hot.contains(t) && *t != choice.0 && !self.stack.iter().any(|(s, _)| s == t)
            })
        })
    }

    /// Types with an untried variant or an unreached field.
    fn frontier(&self) -> impl Iterator<Item = TypeKey> + '_ {
        self.enums
            .iter()
            .chain(self.structs.iter())
            .filter(|(_, t)| t.untaken().is_some())
            .map(|(ty, _)| *ty)
    }

    fn recompute_hot(&mut self) {
        let mut hot: BTreeSet<TypeKey> = self.frontier().collect();
        loop {
            let before = hot.len();
            for ((from, _), to) in &self.edges {
                if !hot.contains(from) && to.iter().any(|t| hot.contains(t)) {
                    hot.insert(*from);
                }
            }
            if hot.len() == before {
                break;
            }
        }
        self.hot = hot;
    }

    fn soft(&self) -> bool {
        self.stack.len() < SOFT_DEPTH
    }
}

/// The invented-value deserializer. `in_key` is set while a map key is being
/// deserialized, so a unit variant there is recorded as a key.
struct Tracer<'s> {
    st: &'s mut TraceState,
    in_key: bool,
}

impl<'s> Tracer<'s> {
    fn root(st: &'s mut TraceState) -> Self {
        Self { st, in_key: false }
    }
}

macro_rules! invent {
    ($($method:ident => $visit:ident($($value:expr)?);)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, TraceError> {
            visitor.$visit($($value)?)
        }
    )*};
}

impl<'de> Deserializer<'de> for Tracer<'_> {
    type Error = TraceError;

    invent! {
        // Custom impls that ask for "anything" are leaves; a string is what
        // the wire's ones (base64, text-or-base64) expect.
        deserialize_any => visit_str("");
        deserialize_bool => visit_bool(false);
        deserialize_i8 => visit_i64(0);
        deserialize_i16 => visit_i64(0);
        deserialize_i32 => visit_i64(0);
        deserialize_i64 => visit_i64(0);
        deserialize_i128 => visit_i64(0);
        deserialize_u8 => visit_u64(0);
        deserialize_u16 => visit_u64(0);
        deserialize_u32 => visit_u64(0);
        deserialize_u64 => visit_u64(0);
        deserialize_u128 => visit_u64(0);
        deserialize_f32 => visit_f32(0.0);
        deserialize_f64 => visit_f64(0.0);
        deserialize_char => visit_char('a');
        deserialize_str => visit_str("");
        deserialize_string => visit_str("");
        deserialize_bytes => visit_bytes(&[]);
        deserialize_byte_buf => visit_bytes(&[]);
        deserialize_unit => visit_unit();
        deserialize_identifier => visit_str("");
        deserialize_ignored_any => visit_unit();
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, TraceError> {
        if self.st.soft() {
            visitor.visit_some(self)
        } else {
            visitor.visit_none()
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, TraceError> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, TraceError> {
        if name == RAW_VALUE_TOKEN {
            return visitor.visit_map(RawValueEntry { step: 0 });
        }
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, TraceError> {
        let left = usize::from(self.st.soft());
        visitor.visit_seq(Elements { st: self.st, left })
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, TraceError> {
        visitor.visit_seq(Elements {
            st: self.st,
            left: len,
        })
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, TraceError> {
        visitor.visit_seq(Elements {
            st: self.st,
            left: len,
        })
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, TraceError> {
        let left = usize::from(self.st.soft());
        visitor.visit_map(Entries { st: self.st, left })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, TraceError> {
        let ty = (name, fields.as_ptr() as usize);
        let start = self.st.struct_start(ty, fields);
        self.st.enter(ty, start)?;
        let r = visitor.visit_map(Fields::new(&mut *self.st, fields, start));
        self.st.leave();
        r
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, TraceError> {
        if variants.is_empty() {
            return Err(TraceError::new("an enum with no variants"));
        }
        let ty = (name, variants.as_ptr() as usize);
        let pick = self.st.pick_variant(ty, variants.len());
        self.st.enter(ty, pick)?;
        let r = visitor.visit_enum(Variant {
            st: &mut *self.st,
            name: variants[pick],
            in_key: self.in_key,
        });
        self.st.leave();
        r
    }
}

/// A struct's (or struct variant's) fields, one invented value each, in a
/// rotated order.
struct Fields<'s> {
    st: &'s mut TraceState,
    fields: &'static [&'static str],
    start: usize,
    next: usize,
    current: usize,
}

impl<'s> Fields<'s> {
    fn new(st: &'s mut TraceState, fields: &'static [&'static str], start: usize) -> Self {
        Self {
            st,
            fields,
            start,
            next: 0,
            current: 0,
        }
    }
}

impl<'de> MapAccess<'de> for Fields<'_> {
    type Error = TraceError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, TraceError> {
        if self.next >= self.fields.len() {
            return Ok(None);
        }
        self.current = (self.start + self.next) % self.fields.len();
        self.next += 1;
        seed.deserialize(StrDeserializer::new(self.fields[self.current]))
            .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, TraceError> {
        self.st.at_field(self.current);
        seed.deserialize(Tracer {
            st: &mut *self.st,
            in_key: false,
        })
    }
}

/// A sequence or tuple of `left` invented elements.
struct Elements<'s> {
    st: &'s mut TraceState,
    left: usize,
}

impl<'de> SeqAccess<'de> for Elements<'_> {
    type Error = TraceError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, TraceError> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(Tracer {
            st: &mut *self.st,
            in_key: false,
        })
        .map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.left)
    }
}

/// A map of `left` invented entries.
struct Entries<'s> {
    st: &'s mut TraceState,
    left: usize,
}

impl<'de> MapAccess<'de> for Entries<'_> {
    type Error = TraceError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, TraceError> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(Tracer {
            st: &mut *self.st,
            in_key: true,
        })
        .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, TraceError> {
        seed.deserialize(Tracer {
            st: &mut *self.st,
            in_key: false,
        })
    }
}

/// The one entry serde_json's `RawValue` asks for: its token, then the text.
struct RawValueEntry {
    step: u8,
}

impl<'de> MapAccess<'de> for RawValueEntry {
    type Error = TraceError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, TraceError> {
        if self.step > 0 {
            return Ok(None);
        }
        self.step = 1;
        seed.deserialize(StrDeserializer::new(RAW_VALUE_TOKEN))
            .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, TraceError> {
        seed.deserialize(StrDeserializer::new("null"))
    }
}

/// The variant a trace picked; what the derived visitor does with it says
/// its shape.
struct Variant<'s> {
    st: &'s mut TraceState,
    name: &'static str,
    in_key: bool,
}

impl<'de, 's> EnumAccess<'de> for Variant<'s> {
    type Error = TraceError;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self), TraceError> {
        let tag = seed.deserialize(StrDeserializer::new(self.name))?;
        Ok((tag, self))
    }
}

impl<'de> VariantAccess<'de> for Variant<'_> {
    type Error = TraceError;

    fn unit_variant(self) -> Result<(), TraceError> {
        if self.in_key {
            self.st.key(self.name);
        } else {
            self.st.value(self.name);
        }
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, TraceError> {
        self.st.key(self.name);
        seed.deserialize(Tracer {
            st: self.st,
            in_key: false,
        })
    }

    fn tuple_variant<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, TraceError> {
        self.st.key(self.name);
        visitor.visit_seq(Elements {
            st: self.st,
            left: len,
        })
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, TraceError> {
        self.st.key(self.name);
        let ty = (self.name, fields.as_ptr() as usize);
        let start = self.st.struct_start(ty, fields);
        self.st.enter(ty, start)?;
        let r = visitor.visit_map(Fields::new(&mut *self.st, fields, start));
        self.st.leave();
        r
    }
}

/// Why a traced subtree failed. Expected, and discarded.
#[derive(Debug)]
struct TraceError(String);

impl TraceError {
    fn new(why: &str) -> Self {
        Self(why.to_string())
    }
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl de::StdError for TraceError {}

impl de::Error for TraceError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_envelope_and_the_lens_vocabulary() {
        let names = trace_wire_names();
        // Envelope fields, externally tagged variants, and unit variants.
        for key in ["id", "msg", "projectRead", "events", "hello", "heartbeat"] {
            assert!(names.keys.contains(key), "key {key}");
        }
        for value in ["hello", "stopAllProjects"] {
            assert!(names.values.contains(value), "value {value}");
        }
        assert!(names.structs > 50 && names.enums > 20, "{names:?}");
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(trace_wire_names(), trace_wire_names());
    }
}
