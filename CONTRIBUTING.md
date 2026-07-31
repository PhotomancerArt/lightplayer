# Contributing to LightPlayer

LightPlayer is currently a small project, and this file keeps the contribution
rules simple and explicit while the project is young.

## License

LightPlayer-owned code is licensed under the GNU Affero General Public License
version 3 or later (`AGPL-3.0-or-later`). Third-party code, vendored forks, and
dependencies remain under their own licenses.

## Contributor License Agreement

LightPlayer is dual-licensed: the code is published under `AGPL-3.0-or-later`,
and the maintainer separately offers it under commercial terms. To keep that
possible, outside contributions require a signed Contributor License
Agreement.

The deal, in plain terms: **you keep ownership of your work**, it will always
remain available under an OSI-approved open-source license, and you grant the
maintainer the right to also license it commercially. The full text is short
and worth reading: [docs/cla/individual-cla.md](docs/cla/individual-cla.md).

Signing is automatic — on your first pull request, the CLA bot will prompt
you, and you sign by posting a comment. The signature is recorded once against
your GitHub account and covers future contributions.

If you are contributing as part of your job (your employer owns the rights to
your work), your employer needs to execute the
[Corporate CLA](docs/cla/corporate-cla.md) instead — contact
<photomancerart@gmail.com>.

Additionally, sign off each commit (`git commit -s`) to certify, per the
[Developer Certificate of Origin](https://developercertificate.org/), that you
have the right to submit it. If any part of a contribution is not your
original work, say so in the PR, with its source and license — see
[docs/adr/2026-07-29-license-provenance-discipline.md](docs/adr/2026-07-29-license-provenance-discipline.md)
for the project's provenance rules. See
[docs/adr/2026-07-31-contributor-license-agreement.md](docs/adr/2026-07-31-contributor-license-agreement.md)
for why this process exists.

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
