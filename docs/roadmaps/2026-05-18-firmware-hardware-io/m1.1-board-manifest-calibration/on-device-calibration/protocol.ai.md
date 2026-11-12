# GPIO Calibration Protocol

The ESP32-C6 calibration firmware is host-driven and line-oriented. `lp-cli` owns session state and
firmware only performs the requested pin action.

## Host Commands

```text
HELLO
PING
PULSE <gpio>
STOP
```

`PULSE <gpio>` starts a simple square wave on the requested HAL GPIO and keeps it active until
`STOP` or another `PULSE` command arrives.

## Firmware Events

```text
CAL READY target=esp32c6
CAL PONG
CAL OPEN gpio=<n>
CAL PULSE gpio=<n>
CAL STOP gpio=<n>
CAL ERR <message>
```

The host treats missing `CAL OPEN` / `CAL PULSE` output for the active GPIO within the configured
timeout as a likely crash or dangerous pin and asks the user before writing a reserved reason to the
manifest.
