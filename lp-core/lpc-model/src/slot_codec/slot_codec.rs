use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

use crate::{MapSlot, OptionSlot, SlotValue, ValueSlot};

use super::{
    SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource, ValueReader,
    read_lp_value, write_lp_value,
};

/// Type-owned slot serialization and deserialization.
///
/// A `SlotCodec` implementation consumes exactly one [`ValueReader`] when
/// reading and emits exactly one [`SlotValueWriter`] when writing. It does not
/// see raw JSON, raw TOML, or a materialized syntax tree.
pub trait SlotCodec: Sized {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource;

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite;

    fn should_write_slot(&self) -> bool {
        true
    }
}

impl<T> SlotCodec for ValueSlot<T>
where
    T: SlotValue,
{
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        let shape = T::value_shape();
        let value = read_lp_value(&shape.ty, value)?;
        let value =
            T::from_lp_value(&value).map_err(|err| SyntaxError::new("", None, err.to_string()))?;
        Ok(Self::new(value))
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        let shape = T::value_shape();
        write_lp_value(value, &shape.ty, &self.value().to_lp_value())
    }
}

impl<V> SlotCodec for MapSlot<String, V>
where
    V: SlotCodec,
{
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        value
            .string_key_map(V::read_slot)
            .map(MapSlot::<String, V>::new)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        write_string_map(value, &self.entries)
    }

    fn should_write_slot(&self) -> bool {
        !self.entries.is_empty()
    }
}

impl<V> SlotCodec for MapSlot<u32, V>
where
    V: SlotCodec,
{
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        value.u32_key_map(V::read_slot).map(MapSlot::<u32, V>::new)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        value.u32_key_map(&self.entries, |value, item| item.write_slot(value))
    }

    fn should_write_slot(&self) -> bool {
        !self.entries.is_empty()
    }
}

impl<T> SlotCodec for OptionSlot<T>
where
    T: SlotCodec,
{
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        T::read_slot(value).map(OptionSlot::some)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        match &self.data {
            Some(data) => data.write_slot(value),
            None => Err(SlotWriteError::Serialize),
        }
    }

    fn should_write_slot(&self) -> bool {
        self.data.is_some()
    }
}

fn write_string_map<W, V>(
    value: SlotValueWriter<'_, W>,
    map: &BTreeMap<String, V>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
    V: SlotCodec,
{
    value.string_key_map(map, |value, item| item.write_slot(value))
}

impl SlotCodec for crate::GlslOpts {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        const FIELDS: &[&str] = &["add_sub", "mul", "div"];
        let mut out = Self::default();
        let mut object = value.object()?;
        while let Some(mut prop) = object.next_prop()? {
            match prop.name() {
                "add_sub" => out.add_sub = SlotCodec::read_slot(prop.value())?,
                "mul" => out.mul = SlotCodec::read_slot(prop.value())?,
                "div" => out.div = SlotCodec::read_slot(prop.value())?,
                other => return Err(prop.unknown_field(other, FIELDS)),
            }
        }
        Ok(out)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        let mut object = value.object()?;
        if self.add_sub.should_write_slot() {
            self.add_sub.write_slot(object.prop("add_sub")?)?;
        }
        if self.mul.should_write_slot() {
            self.mul.write_slot(object.prop("mul")?)?;
        }
        if self.div.should_write_slot() {
            self.div.write_slot(object.prop("div")?)?;
        }
        object.finish()
    }
}
