ser-write
=========

[![Crate][Crate img]][Crate Link]
[![Docs][Docs img]][Docs Link]
[![Build Status][Build img]][Build Link]
[![Coverage Status][Coverage img]][Coverage Link]
[![Minimum rustc version][rustc version img]][rustc version link]

Writer-style serializers and deserializers for convenience designed with embedded (`no_std`) targets in mind.

* Writer-style serializers use the common writer trait found in this crate.
* Designed for `no_std`.
* Fully supports `std` or `alloc` when enabled for code portability and testablilty.
* For each serializer a deserializer is provided for convenience.
* Embedded projects can implement `SerWrite` trait for custom containers, frame builders and more.

This crate provides:

* the trait - `SerWrite` which should be used by serializers to write the serialized output,
* `SerError` - a convenient error type,
* `SliceWriter` - a convenient slice writer object implementing `SerWrite`,
* `SerWrite` implementations for foreign types.

Depending on the enabled crate features, `SerWrite` is implemented for:

* `SliceWriter` - example slice writer implementation,
* [`arrayvec::ArrayVec<u8,CAP>`](https://crates.io/crates/arrayvec) - `arrayvec` feature,
* [`heapless::Vec<u8,CAP>`](https://crates.io/crates/heapless) - `heapless` feature,
* [`smallvec::SmallVec<[u8; CAP]>`](https://crates.io/crates/smallvec) - `smallvec` feature,
* [`tinyvec::ArrayVec<[u8; CAP]>`](https://crates.io/crates/tinyvec) - `tinyvec` feature,
* [`tinyvec::SliceVec<'_, u8>`](https://crates.io/crates/tinyvec) - `tinyvec` feature,
* [`tinyvec::TinyVec<[u8; CAP]>`](https://crates.io/crates/tinyvec) - `tinyvec` with `alloc` or `std` feature,
* `Vec<u8>` - `alloc` or `std` feature,
* `VecDeque<u8>` - `alloc` or `std` feature,
* `io::Cursor<T: io::Write>` - `std` feature,

`smallvec` also enables `alloc`.


Usage
-----

Start by adding a dependency to the serializer:

For example:

```
[dependencies]
ser-write-json = { version = "0.3", default-features = false }
```

If you want to also pull implementations of `SerWrite` for the foreign types add:

```
ser-write = { version = "0.3", default-features = false, features = ["arrayvec", "heapless"] }
```

In the above example implementations for: `arrayvec::ArrayVec<u8;_>` and `heapless::Vec<u8>` are selected.


Serializers
-----------

Currently available serializers and deserializers are:

* [JSON](https://json.org) (compact) - [ser-write-json](ser-write-json/)
* [MessagePack](https://msgpack.org) - [ser-write-msgpack](ser-write-msgpack/)


Example
-------

An example `SliceWriter` implementation:

```rs
use ser_write::{SerWrite, SerResult, SerError};

#[derive(Debug, PartialEq)]
pub struct SliceWriter<'a> {
    pub buf: &'a mut [u8],
    pub len: usize
}

impl SerWrite for SliceWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        let end = self.len + buf.len();
        match self.buf.get_mut(self.len..end) {
            Some(chunk) => {
                chunk.copy_from_slice(buf);
                self.len = end;
                Ok(())
            }
            None => Err(SerError::BufferFull)
        }
    }
}
```


Alternatives
------------

For `alloc` only:
* [serde_json](https://crates.io/crates/serde_json)

Alternatively there's a Rust Embedded Community crate for serializeing JSONs without `std`:

* [serde-json-core](https://crates.io/crates/serde-json-core)
* [serde-json-core-fmt](https://crates.io/crates/serde-json-core-fmt) (a writer-style attempt abusing `fmt::Display` trait)

`serde-json-core` is a true `no_std`, no `alloc` alternative but rather inconvenient. One has to serialize data into intermediate slices instead just pushing data to outgoing buffers or frame builder implementations.


Why though?
-----------

* This crate would not be needed once something like `io::Write` lands in the Rust core.


Rust Version Requirements
-------------------------

`ser-write` requires Rustc version 1.81 or greater.

[Crate Link]: https://crates.io/crates/ser-write
[Crate img]: https://img.shields.io/crates/v/ser-write.svg
[Docs Link]: https://docs.rs/ser-write
[Docs img]: https://docs.rs/ser-write/badge.svg
[Build Link]: https://github.com/royaltm/rust-ser-write/actions/workflows/rust.yml
[Build img]: https://github.com/royaltm/rust-ser-write/actions/workflows/rust.yml/badge.svg?branch=main
[rustc version link]: https://github.com/royaltm/rust-ser-write#rust-version-requirements
[rustc version img]: https://img.shields.io/badge/rustc-1.81+-lightgray.svg
[Coverage Link]: https://coveralls.io/github/royaltm/rust-ser-write?branch=main
[Coverage img]: https://coveralls.io/repos/github/royaltm/rust-ser-write/badge.svg?branch=main