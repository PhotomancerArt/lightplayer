# True Paged Static Catalog Reads

## Status

Implemented. `snapshot_page_with_static_catalog()` now merges generated static
catalog ids and dynamic registry entries directly, applying `after` and `limit`
without constructing a full owned catalog first.

## Smell

The old `snapshot_page_with_static_catalog()` implementation built a full owned
static-catalog snapshot and then paged that snapshot.

This makes paged project shape reads look memory-conscious while still paying
the full catalog allocation cost up front.

## Better Shape

Replace page construction with a merged iterator over:

- generated static catalog ids
- dynamic registry entries

The iterator should apply the `after` cursor and `limit` without first building
an owned `BTreeMap` of every static shape.

## Useful Context

- `SlotShapeRegistry::snapshot_page_with_static_catalog`
- direct static-catalog/dynamic-registry merge logic in
  `SlotShapeRegistry::snapshot_page_with_static_catalog`
- project read shape paging in `lpc-engine`

## Remaining Risk

No-limit, non-streaming shape reads in low-memory builds are intentionally
bounded to one page. Clients that want the complete catalog should use the
streaming path or continue paging until `complete` is true.
