ser-write-json
==============

[![Crate][Crate img]][Crate Link]
[![Docs][Docs img]][Docs Link]
[![Build Status][Build img]][Build Link]
[![Coverage Status][Coverage img]][Coverage Link]
[![Minimum rustc version][rustc version img]][rustc version link]

This crate provides a `no_std` friendly [JSON](https://json.org) compact serializer for [serde](https://crates.io/crates/serde) using [`SerWrite`] as a writer and a deserializer for convenience.

This crate has been in some parts derived from work of [serde-json-core](https://crates.io/crates/serde-json-core) and [serde_json](https://crates.io/crates/serde_json).


Usage
-----

```
[dependencies]
ser-write-json = { version = "0.3", default-features = false }
```


Serializer
----------

`ser-write-json` provides JSON serializers in 4 flavors depending on how do you want to handle types serialized with `serialize_bytes` method.

* `to_writer` - serialize bytes as number arrays,
* `to_writer_hex_bytes` - as HEX-encoded strings,
* `to_writer_base64_bytes` - as Base64 encoded strings,
* `to_writer_pass_bytes` - passing through bytes to a writer assuming they contain pre-serialized JSON fragments.
* `to_writer_with_encoder` - a custom encoder can be provided.

Custom string encoders can be implemented using `ByteEncoder` trait. There's an [example](examples/) in this repository that does exactly that.

Features:

* `std` enables std library,
* `alloc` enables alloc library,

With `std` or `alloc` features enabled additional `to_string...`  methods are provided for convenience.


Deserializer
------------

Unlike most JSON deserializers, a deserializer in `ser-write-json` expects a JSON encoded **mutable slice** of bytes. `&str` or `&[u8]` types are deserialized using (ZERO-COPY) references from the provided slice. The slice needs to be mutable so the decoder can unescape JSON strings and decode bytes from strings in various formats in-place.

The JSON deserializer is available in 4 flavors depending on how do you want to handle types deserialized with `deserialize_bytes` method from JSON strings:

* `from_mut_slice` - decodes bytes from regular JSON strings without checking if they are proper UTF-8 strings,
* `from_mut_slice_hex_bytes` - expect two hexadecimal ASCII characters per byte,
* `from_mut_slice_base64_bytes` - expect Base64 encoded string,
* `from_mut_slice_with_decoder` - a custom decoder can be provided.

`Deserializer` deserializes bytes in-place from a JSON array of numbers regardless of the chosen implementation.

`Deserializer` supports self-describing formats.

`Deserializer` deserializes structs from both JSON objects and arrays.

Features:

* `de-any-f32` deserialization of floats to *any* (self-describing) type will deserialize to `f32` instead of `f64`.


Rust Version Requirements
-------------------------

`ser-write-json` requires Rustc version 1.81 or greater.

[`SerWrite`]: https://docs.rs/ser-write/latest/ser_write/trait.SerWrite.html
[Crate Link]: https://crates.io/crates/ser-write-json
[Crate img]: https://img.shields.io/crates/v/ser-write-json.svg
[Docs Link]: https://docs.rs/ser-write-json
[Docs img]: https://docs.rs/ser-write-json/badge.svg
[Build Link]: https://github.com/royaltm/rust-ser-write/actions/workflows/rust.yml
[Build img]: https://github.com/royaltm/rust-ser-write/actions/workflows/rust.yml/badge.svg?branch=main
[rustc version link]: https://github.com/royaltm/rust-ser-write/tree/main/ser-write-json#rust-version-requirements
[rustc version img]: https://img.shields.io/badge/rustc-1.81+-lightgray.svg
[Coverage Link]: https://coveralls.io/github/royaltm/rust-ser-write?branch=main
[Coverage img]: https://coveralls.io/repos/github/royaltm/rust-ser-write/badge.svg?branch=main