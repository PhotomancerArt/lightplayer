# Phase 6: Implement Report Generation (TOML)

## Description

Implement TOML report generation for overall report and per-test statistics. Include metadata (git hash, timestamp) and statistics summaries.

## Implementation

- Create `src/report.rs`
- Define TOML-serializable structs:
  - `ReportMetadata` (git_hash, timestamp, test_count)
  - `OverallReport` (metadata, summary, tests)
  - `TestReport` (name, before, after, delta, functions)
  - `FunctionReport` (name, before, after, delta)
  - Use `serde` and `toml` for serialization
- Implement `collect_git_hash()`:
  - Runs `git rev-parse HEAD` command
  - Returns hash string or "unknown" on failure
- Implement `generate_report()`:
  - Takes overall stats and test summaries
  - Creates `ReportMetadata` with git hash, timestamp, test count
  - Serializes to TOML
  - Writes `report.toml` to report directory
- Implement `generate_test_report()`:
  - Takes test name, before/after stats, function breakdowns
  - Creates `TestReport` struct
  - Serializes to TOML
  - Writes `stats.toml` to test directory
- Handle timestamp formatting (ISO 8601)

## Success Criteria

- TOML reports are generated correctly
- Metadata includes git hash and timestamp
- Statistics are serialized properly
- Report structure matches design
- Code compiles without errors
