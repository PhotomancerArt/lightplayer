# lp-server

The LightPlayer server module, handles communication, project management for a LightPlayer server.

Used by apps and firmwares to provide lightplayer server functionality.

`no_std`, designed for embedding. All communications are abstracted (no serial or http here, that's
handled by the apps).