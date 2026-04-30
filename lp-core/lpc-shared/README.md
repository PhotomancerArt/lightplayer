# lpc-shared

Shared lightplayer code for use by `lpc-engine`, `lpa-server` and other embeddable portions of
Lightplayer.

`no_std`, designed for running on embedded devices.

Contains support code for the various LightPlayer modules, like logging, file IO, etc.

Does _not_ include core model/source/wire definitions, which live in
`lpc-model`, `lpc-source`, and `lpc-wire`.