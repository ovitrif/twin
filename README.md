# Twin

Your Android phone and your iPhone on your Mac, one at a time or side by side in one
window, from a play button in the menu bar. Pair Android phones over Wi-Fi with a QR code,
mirror and drive them with your mouse and keyboard, and launch Android Virtual Devices,
without remembering SDK commands. Nothing is installed on either phone.

<p align="center">
  <img src="assets/popover.webp" alt="Twin's menu bar popover moving between its device list, settings and QR pairing pages" width="380">
</p>

- **Android:** Twin wraps Android's wireless debugging flow behind a QR code. Scan it
  with your phone and Twin handles mDNS discovery, `adb pair`, `adb connect`, and
  reconnects, then mirrors the phone with [scrcpy](https://github.com/Genymobile/scrcpy).
- **iPhone:** on iOS 27 with Developer Mode, Twin mirrors a USB-connected iPhone through
  the Device Hub services Xcode uses, so its screen keeps its real status bar and clicks,
  drags, scrolls and typing reach it live.
- **Side by side:** with both connected, one play button shows the two phones in one
  window that moves as one; each phone can also open or hide on its own.

Twin runs on Apple silicon Macs; the `twin` CLI also runs on Linux, for Android.

## Install

```sh
curl -fsSL https://github.com/ovitrif/twin/releases/latest/download/install.sh | sh
```

Pin a release or choose the install directory:

```sh
curl -fsSL https://github.com/ovitrif/twin/releases/latest/download/install.sh | \
  TWIN_INSTALL_TAG=vX.Y.Z TWIN_INSTALL_DIR="$HOME/.local/bin" sh
```

`install.sh` installs the `twin` CLI and, on macOS, `Twin.app` into /Applications.

What Twin needs, which its Settings check under Setup:

- **Android phones:** nothing more. Twin.app brings its own `adb`; the phone needs
  Android 11+ with developer options on, on the same Wi-Fi network as your Mac (or USB).
- **iPhones:** Xcode 27 from the App Store, opened once (it sets up the phone's developer
  services), and an iPhone on USB, on iOS 27+ with Developer Mode on, that trusts this Mac.
- **Emulators (optional):** the Android SDK Emulator, from Android Studio; deleting them
  needs its Command-Line Tools (`avdmanager`).
- **The `twin` CLI's own mirror (optional):** `scrcpy` on your PATH (`brew install scrcpy`).

## Menu bar app

```sh
twin app
```

Twin's icon appears in the menu bar. Left-click toggles the popover: a Devices tab
listing your phones (with one-click mirroring, side by side when Settings say so) and configured emulators, a Logs tab,
Settings, and QR pairing for new phones. Refresh, Settings, and Pair stay in the top-right
corner on every page. Each configured AVD stays listed while stopped, starting, or running.
Click the play icon to open its native Android Emulator window, or the trash icon to delete
a stopped AVD and its saved data after confirmation. Both actions are disabled while the
emulator is starting or running. Right-click (or control-click) offers Show, Pair, Refresh,
and Quit.

The popover works from the keyboard: Tab moves between controls, Return or Space activates
the focused one, Escape closes the popover, and ← / → step back and forward through the
pages you visited, then through Main, Settings, and Pair in order.

Twin.app holds the menu bar app, `Twin Mirror.app` (every mirror window: an Android phone,
an iPhone, or both side by side) and Google's `adb` under `Contents/Helpers`. Twin uses your
own `adb` when you have one installed, so one adb server serves both.

## CLI

```sh
twin
```

Running `twin` with no arguments checks ADB, shows a QR code, and guides the phone from
`Developer options -> Wireless debugging -> Pair device with QR code` to a connected
device, with live waits, retries, and manual IP:port fallbacks when discovery misbehaves.
Once connected it offers to start scrcpy.

Useful commands and flags:

```sh
twin status --json       # ADB, scrcpy, Android phones and iPhones, for scripts and UIs
twin reset-adb           # kill and restart the local ADB server
twin completions zsh     # print zsh completions
twin app                 # launch the menu bar app
twin --connect-only      # pair and connect without the scrcpy menu
twin --background        # start scrcpy detached and exit
twin --foreground        # start scrcpy and wait until it exits
twin --stable            # scrcpy + keepalive watch + reconnects + Wi-Fi diagnostics
twin --watch --wifi-doctor
twin --timeout 120       # wait longer than the 60-second default
twin --device-serial SERIAL --background
twin --window-title "Android" --window-height 1071
twin --plain-window --always-on-top
twin --adb /path/to/adb --scrcpy /path/to/scrcpy
twin iphone status       # iPhones on USB
twin iphone mirror       # mirror the first one; --udid picks another
twin iphone devices --serial SERIAL   # that Android phone beside the iPhone
```

The `twin` CLI mirrors with `scrcpy` from your PATH: borderless at 480x1071 with
`--stay-awake` and no audio by default; pass `--plain-window` for scrcpy's regular decorated
window. The menu bar app needs no scrcpy: its mirrors run scrcpy's server on the phone and
draw it in Twin Mirror. It stores its settings in `~/.config/twin/config.toml`.

## iPhone mirroring

The iPhone's screen and touch travel over one Device Hub session through `dmb-hid`: an
H.265 stream of the screen and CoreDevice's HID service for touch, the services Xcode 27's
Device Hub uses. These are private, reverse-engineered services, so an iOS update can
break them. Before iOS 27 Twin falls back to QuickTime's capture path, view-only, which
asks once for camera access.

In the window:

- Click, drag and scroll to touch the phone; type to type into it.
- ⌘H goes home, ⌘L locks, ⌘B or right-click goes back on Android, ⌘W closes the mirror.
- Hold ⌘ to see the title bar; ⌘ + drag anywhere moves the window.
- Drag any edge of the phone's outline to resize; the window keeps the phone's aspect and
  turns with it.

## Build from source

```sh
git clone https://github.com/ovitrif/twin.git
cd twin
cargo build --release            # target/release/twin and target/release/twin-app
scripts/bundle-app.sh            # target/bundle/Twin.app, with Twin Mirror (needs Xcode) and adb
(cd apple && swift test)         # Twin Mirror's tests
```

The Rust workspace has three crates: `twin-core` (ADB, scrcpy, QR, and Bonjour logic),
`twin` (the CLI), and `twin-app` (the menu bar app, built from the design in `DESIGN.pen`).
`apple/` holds Twin Mirror, the Swift app that draws every mirror window (with a Swift
port of scrcpy's client for Android),
its `dmb` helper CLI, and `apple/hid` (`dmb-hid`, on
[idevice](https://github.com/jkcoxson/idevice)), which carries the iPhone's screen and
touch.

## License

MIT. Bundled [Inter](https://rsms.me/inter/) font is licensed under the SIL OFL 1.1;
icons are [Phosphor](https://phosphoricons.com/) (MIT); Twin's cast mark is
[theSVG](https://thesvg.org)'s Cast icon (MIT). scrcpy and its server are Apache-2.0.
