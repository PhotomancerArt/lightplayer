//! Ergonomic, context-resolved readers for generated slot views.
//!
//! Generated `*View` types store compiled [`SlotAccessor`] values. Reader
//! wrappers are the public surface returned by those views: node/runtime code
//! can ask a field to resolve itself through the active context without knowing
//! how slot accessors are compiled or how optional payloads are addressed.

use crate::{FromLpValue, SlotAccessor, SlotPath};

/// Context capable of resolving a generated slot reader.
///
/// `lpc-model` owns the view and reader types, while the engine owns binding
/// resolution. This trait is the small bridge between those layers.
pub trait SlotReadContext {
    type Error;

    /// Resolve a compiled accessor as a typed value.
    fn read_slot_value<T>(&mut self, accessor: &SlotAccessor) -> Result<T, Self::Error>
    where
        T: FromLpValue;

    /// True when an error means an optional slot has no active payload.
    fn is_optional_none_error(error: &Self::Error) -> bool {
        let _ = error;
        false
    }
}

/// Reader for a generated slot field.
///
/// This may point at a value leaf or an aggregate slot. Calling [`Self::get`]
/// expects the field to resolve to a value leaf.
#[derive(Clone, Copy, Debug)]
pub struct SlotFieldReader<'a> {
    accessor: &'a SlotAccessor,
}

impl<'a> SlotFieldReader<'a> {
    pub const fn new(accessor: &'a SlotAccessor) -> Self {
        Self { accessor }
    }

    pub fn accessor(&self) -> &'a SlotAccessor {
        self.accessor
    }

    pub fn path(&self) -> &SlotPath {
        self.accessor.path()
    }

    pub fn get<C, T>(&self, ctx: &mut C) -> Result<T, C::Error>
    where
        C: SlotReadContext,
        T: FromLpValue,
    {
        ctx.read_slot_value(self.accessor)
    }
}

/// Reader for an optional generated slot field.
#[derive(Clone, Copy, Debug)]
pub struct SlotOptionReader<'a> {
    accessor: &'a SlotAccessor,
    some_accessor: &'a SlotAccessor,
}

impl<'a> SlotOptionReader<'a> {
    pub const fn new(accessor: &'a SlotAccessor, some_accessor: &'a SlotAccessor) -> Self {
        Self {
            accessor,
            some_accessor,
        }
    }

    pub fn accessor(&self) -> &'a SlotAccessor {
        self.accessor
    }

    pub fn some_accessor(&self) -> &'a SlotAccessor {
        self.some_accessor
    }

    pub fn path(&self) -> &SlotPath {
        self.accessor.path()
    }

    pub fn some_path(&self) -> &SlotPath {
        self.some_accessor.path()
    }

    pub fn get<C, T>(&self, ctx: &mut C) -> Result<Option<T>, C::Error>
    where
        C: SlotReadContext,
        T: FromLpValue,
    {
        match ctx.read_slot_value(self.some_accessor) {
            Ok(value) => Ok(Some(value)),
            Err(err) if C::is_optional_none_error(&err) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub fn get_or<C, T>(&self, ctx: &mut C, default: T) -> Result<T, C::Error>
    where
        C: SlotReadContext,
        T: FromLpValue,
    {
        self.get(ctx).map(|value| value.unwrap_or(default))
    }
}
