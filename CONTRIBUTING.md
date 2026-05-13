# Contributing to LightPlayer

LightPlayer is currently a small project, and this file keeps the contribution
rules simple and explicit while the project is young.

## License

LightPlayer-owned code is licensed under the GNU Affero General Public License
version 3 or later (`AGPL-3.0-or-later`). Third-party code, vendored forks, and
dependencies remain under their own licenses.

## Contribution Terms

By submitting a contribution to LightPlayer, you agree that:

- You have the right to submit the contribution.
- Your contribution is licensed under `AGPL-3.0-or-later`.
- You grant the LightPlayer maintainer a perpetual, worldwide, non-exclusive,
  royalty-free license to use, modify, distribute, sublicense, and relicense
  your contribution as part of LightPlayer, including under alternative
  commercial license terms.

This is intended as lightweight project hygiene, not a heavyweight legal
process. If the project grows to need a formal CLA, these terms can be replaced
with a more explicit agreement before accepting larger outside contributions.

## Development

Before opening a pull request, run the relevant checks for the area you touched.
For broad changes, prefer:

```bash
just check
just build-ci
just test
```

Avoid `cargo build --workspace` and `cargo test --workspace`; this repository
contains RV32-only firmware crates that do not build for the host target.
