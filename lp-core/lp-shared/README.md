# lp-shared

Shared lightplayer code for use by `lp-engine`, `lp-server` and other embeddable portions of
Lightplayer.

`no_std`, desinged for running on embedded devices.

Contains shared code for the various lightplayer modules, like logging, file IO, etc.

Does _not_ include the application model (config, state) which is in `lp-model`