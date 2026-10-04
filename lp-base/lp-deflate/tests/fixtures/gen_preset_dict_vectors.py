#!/usr/bin/env python3
"""Generates this directory's preset-dictionary round-trip fixtures.

Python's `zlib` (bound to the system's zlib, not our decoder) compresses
raw deflate streams with a preset dictionary directly: `wbits=-15` asks for
raw deflate (no zlib header/trailer), and `zdict=...` is the dictionary — so
this script is the independent "known good" source for what a deflate
stream with history already primed in the window looks like. It is run
once, by hand, to produce the committed fixtures below; it is not part of
the crate's build or test run.

Each case `<name>` writes three files:
- `<name>.dict`   the preset dictionary (`buf[..start]` for `inflate`)
- `<name>.deflate` the compressed stream (`src` for `inflate`)
- `<name>.expect`  the plaintext the stream must decode to (`buf[start..]`)

The one exception is `firmware-sector-*`, which writes only `.deflate`: its
dictionary and expected plaintext are just slices of the firmware image
already committed at `lp-fw/bootloaders/`, so the test recomputes them by
slicing that file the same way this script does, rather than duplicating
tens of KB of its bytes into fixtures.

Run from the repo root: `python3 lp-base/lp-deflate/tests/fixtures/gen_preset_dict_vectors.py`
"""

import pathlib
import random
import zlib

HERE = pathlib.Path(__file__).parent
REPO_ROOT = HERE.parents[3]
FIRMWARE = REPO_ROOT / "lp-fw" / "bootloaders" / "esp32c6-bootloader-idf-v5.5.1.bin"


def write_case(name: str, dict_bytes: bytes, payload: bytes, level: int = 6) -> None:
    co = zlib.compressobj(level, zlib.DEFLATED, -15, zdict=dict_bytes)
    compressed = co.compress(payload) + co.flush()

    # Round-trip through zlib itself before committing: this script is the
    # independent oracle, so a bug here must not end up in a fixture.
    do = zlib.decompressobj(-15, zdict=dict_bytes)
    assert do.decompress(compressed) == payload, f"{name}: zlib self-check failed"

    (HERE / f"{name}.dict").write_bytes(dict_bytes)
    (HERE / f"{name}.deflate").write_bytes(compressed)
    (HERE / f"{name}.expect").write_bytes(payload)
    print(f"{name}: dict={len(dict_bytes)} payload={len(payload)} compressed={len(compressed)}")


def firmware_sectors() -> None:
    """The OTA shape itself: the real bootloader image split into 4 KiB
    sectors, each compressed against up to the previous 32 KiB (the whole
    image is far short of 32 KiB, so every sector's dictionary is simply
    "everything before it"). Only the compressed bytes are committed; the
    dictionary and expected plaintext are re-sliced from the firmware file
    by the test, the same way as here, rather than duplicated."""
    data = FIRMWARE.read_bytes()
    chunk = 4096
    for i in range(0, len(data), chunk):
        dict_bytes = data[max(0, i - 32768) : i]
        payload = data[i : i + chunk]
        co = zlib.compressobj(6, zlib.DEFLATED, -15, zdict=dict_bytes)
        compressed = co.compress(payload) + co.flush()
        do = zlib.decompressobj(-15, zdict=dict_bytes)
        assert do.decompress(compressed) == payload, f"firmware sector {i}: zlib self-check failed"
        name = f"firmware-sector-{i // chunk:02d}"
        (HERE / f"{name}.deflate").write_bytes(compressed)
        print(f"{name}: dict={len(dict_bytes)} payload={len(payload)} compressed={len(compressed)}")


def text_small() -> None:
    """A small, human-legible case: matches that reach back into the
    dictionary, not just within the payload."""
    dict_bytes = (b"The quick brown fox jumps over the lazy dog. " * 40)[:1800]
    payload = b"The quick brown fox jumps over the lazy dog, and does it again!"
    write_case("text-small", dict_bytes, payload)


def max_distance() -> None:
    """A match at the largest distance deflate can express (32768): the
    dictionary is exactly a full window, built so the only place the
    payload's leading bytes appear is at its very first byte."""
    rng = random.Random(0xD15_7A7)
    marker = b"MARKER"
    filler = bytes(rng.randrange(256) for _ in range(32768 - len(marker)))
    dict_bytes = marker + filler
    assert len(dict_bytes) == 32768
    payload = marker + b" at the far edge of the window"
    write_case("max-distance", dict_bytes, payload)


def empty_payload() -> None:
    """Zero bytes of real output: an end-of-block-only stream."""
    dict_bytes = b"some dictionary content that goes unused"
    write_case("empty-payload", dict_bytes, b"")


def no_dictionary() -> None:
    """A sanity baseline: an empty dictionary should behave exactly like a
    stream with none at all."""
    write_case("no-dictionary", b"", b"plain payload, no preset dictionary")


if __name__ == "__main__":
    firmware_sectors()
    text_small()
    max_distance()
    empty_payload()
    no_dictionary()
