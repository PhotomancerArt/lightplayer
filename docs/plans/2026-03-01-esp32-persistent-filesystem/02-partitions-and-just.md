# Phase 2: Partitions and just command

## Scope of phase

Add custom partition table (app 3MB, lpfs 1MB) and update just demo-esp32c6-host.

## Implementation Details

- Create `lp-fw/fw-esp32/partitions.csv`
- Update justfile demo-esp32c6-host: add --partition-table partitions.csv to espflash, change dev to upload

## Validate

```bash
# Partitions file exists
test -f lp-fw/fw-esp32/partitions.csv
```
