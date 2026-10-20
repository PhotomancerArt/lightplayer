# The desk's Android test phone

A Pixel 8a kept on the desk for agents to drive with no hands: Studio in the
phone's own Chrome, its own Bluetooth and USB, against desk boards.
`phone.mjs` is the kit (`just phone …`); `walk-ble-phone.mjs` (`just
walk-ble-phone`) and `walk-usb-phone.mjs` (`just walk-usb-phone`) are the
walks on it.

Why a phone and not the Mac: Mac Chrome's Web Bluetooth is not a phone's
(different Bluetooth stack, MTU, connection timing), and the emulated walks
never touch a radio. This phone is Android Chrome — not Bluefy on iOS, which
is still a person's walk.

## One-time setup (hands)

On the phone:

1. Settings › About phone › tap **Build number** seven times.
2. Settings › System › Developer options:
   **USB debugging**, **Wireless debugging**, **Stay awake** on;
   **Enable Bluetooth HCI snoop log** › Enabled.
3. Settings › Security › Screen lock › **None** (a driven page must be
   visible; the kit never types a PIN).
4. Open Chrome once and get past its first-run screens.
5. Wireless debugging › **Pair device with pairing code**, then on the Mac:
   `just phone pair <the six digits>`.

On the Mac: `brew install --cask android-platform-tools`, then `just phone
setup` — it grants Chrome the phone's "Nearby devices" and location
permissions by adb. Without them the Bluetooth chooser opens empty and says
"Chrome needs permission to scan for nearby devices".

USB debugging approved once is enough for Wi‑Fi too: with Wireless debugging
on, `adb connect <ip>:<port>` (`just phone connect` finds it by mDNS) worked
with no pairing code (2026-10-17).

## How it works

- **Wireless debugging, not the cable.** The phone has one USB-C port and a
  Web Serial walk needs it for the board. After pairing, adb finds the phone
  by mDNS (`just phone connect` if it has not).
- **Chrome's DevTools socket** forwarded to a Mac port (`just phone cdp`) is
  the same protocol the headless walks speak: `StudioDriver.attach` drives
  the phone's Chrome like the desk's, and `connectBluetooth` answers the
  phone's real Web Bluetooth chooser over CDP's `DeviceAccess` domain.
- **`adb reverse`** (`just phone serve <port>`) puts a Mac server on the
  phone's own `localhost`, a secure context: Web Bluetooth works there with no
  HTTPS and no tailnet.
- **The Bluetooth packet log** comes off in a bug report:
  `just phone snoop <dir>` writes `btsnoop_hci.log`, which Wireshark opens.
  Enable the snoop log before the session you want in it.
- **`just phone shot <png>`** is the whole screen, system UI and choosers
  included — what CDP's page screenshot cannot see.
- **`just phone ui`** is the screen as uiautomator reads it (text and
  bounds), and `phone.tapText(pattern)` taps a node by what it says: how a
  walk answers a native sheet CDP cannot reach (the WebUSB chooser,
  Android's "Allow Chrome to access …?"). CDP's `DeviceAccess` covers the
  Bluetooth chooser only.
- **`just phone usb`** is the USB device on the phone's port, as Android
  lists it; on a C6 its serial number is the MAC, which names the desk
  board.

## What the first sitting found (2026-10-17)

- **The phone is a Pixel 8a on Android 14 with Chrome 122** (a refurbished
  phone's factory Chrome). It has no Web Serial; updating Chrome is a Play
  Store tap on the phone.
- **Android Chrome's chooser ids are the boards' Bluetooth addresses**: the
  C6's base MAC with its top two bits set. So the walk picks the leased board
  by MAC, whatever project name it advertises.
- **Chrome's DevTools socket takes over 10 s to appear on a cold start**, and
  Chrome in the background renders nothing (a CDP screenshot hangs): `phone
  cdp` brings Chrome to the front every time and waits up to 30 s.
- **Every connect from the phone panicked the C6** until PR #1180: Android
  asks for connection parameters over the link layer, which no Mac or
  iPhone central had done
  (`docs/defects/2026-10-17-an-android-phone-connecting-over-bluetooth-panics-the-c6.md`).
  Fixed in the forks of bt-hci and trouble-host under `third_party/`.
- **The board must let the phone in.** The phone is signed out and holds no
  key, so a board whose access is `open: nobody` (fixture-c6's) shows it as
  locked, and the walk stops at `identify`. CX2 `c6-button` is open.
- **The bug report held only `btsnooz_hci.log`** (the filtered summary) on
  2026-10-17, not `btsnoop_hci.log`: check Developer options › Enable
  Bluetooth HCI snoop log is set to *Enabled*, not *Filtered*, before a
  session you want the packets of.

## The USB walk

Chrome on Android has no wired Web Serial, so Studio's USB square speaks to
a board over WebUSB there (`docs/adr/2026-10-18-android-usb-over-webusb.md`).
The board is plugged into the PHONE's USB-C port, not the desk hub, and is
powered by the phone.

```bash
just walk-usb-phone --serve-release        # this checkout's release bundle (just studio-web-story-build)
just walk-usb-phone --studio https://lightplayer.app/
```

It finds the board by MAC (`phone usb`) and leases it, presses USB, answers
Chrome's chooser and Android's access dialog by tapping what they say, and
checks the card (by MAC). Then it installs firmware from the phone:
Factory reset (esptool's erase over WebUSB), Install firmware on the blank
board (this Studio's build, written by the flasher over WebUSB), the board's
hello on that build, and a project put back from the gallery; then the
editor and Play. **It erases the board** — a test board only; `--no-install`
skips the firmware steps. A Studio tab an earlier walk left open still holds
the board, so the walk closes them first. There is no power cut: the board's
power is the phone's port.

The flasher survives what the C6 drops on a phone (it reads in short,
digest-checked segments it reads again, and proves each written image by
its MD5): `docs/defects/2026-10-18-a-flash-over-webusb-on-a-phone-loses-the-boards-replies.md`.
Its numbers are this phone's: a lab page measured them by driving esptool-js
over the shipped port with `StudioDriver.evaluate` (the method is in the
defect).

## The walk

```bash
just walk-ble-phone                                   # lightplayer.app, fixture-c6
just walk-ble-phone --studio http://127.0.0.1:<port>  # a running studio-dev (debug wasm: slow over Wi-Fi)
just walk-ble-phone --serve-release                   # this checkout's release bundle (just studio-web-story-build)
just walk-ble-phone --board CX2 --name "LP-Peach" --snoop
```

It leases the board for as long as it runs (`board take --pid`), and cuts the
board's power through `board power-cycle`. The board needs a project loaded
and firmware the chosen Studio speaks to. Its output is in
`target/walk-ble-phone/`: a report, the page's console, page screenshots per
step and the phone's whole screen at the end.

Its numbers are a Pixel's Chrome against a C6 at desk distance. Name them
that way: a phone number, not Bluefy's, not a Mac's.
