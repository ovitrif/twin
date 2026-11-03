# imb

iPhone Mirror Bridge for macOS: mirror a USB-connected iPhone in a QuickTime-style
window and use it with your mouse and keyboard, with nothing installed on the phone.

imb reads the screen through the same capture path QuickTime uses and draws it in a
borderless window with the phone's rounded corners, like
[awb](https://github.com/ovitrif/awb)'s Android mirror. On iOS 27 and later, clicks,
drags, scrolls and typing reach the phone live through the CoreDevice HID service
Xcode's Device Hub uses.

## Requirements

- macOS 14+ with Xcode installed (for `devicectl`)
- An iPhone on USB that trusts this Mac; iOS 27+ with Developer Mode on for input

## Use

```sh
imb status          # iPhones on USB
imb mirror          # mirror the first one; --udid picks another
```

The first mirror asks for camera access, because macOS treats the iPhone's screen
as a capture device.

In the window:

- Click, drag and scroll to touch the phone; type to type into it.
- ⌘H goes home, ⌘L locks, ⌘W or ⌘Q closes the mirror.
- ⌘ + drag anywhere moves the window, as does dragging the title bar that appears on hover.
- The window turns with the phone.

For phones before iOS 27, `imb mirror --wda http://127.0.0.1:8100` sends input to a
running [WebDriverAgent](https://github.com/appium/WebDriverAgent) instead (forward
it with `iproxy 8100 8100`); without either, the mirror is view-only.

## Build

```sh
scripts/bundle-app.sh            # build/imb and build/Twin.app (needs cargo for imb-hid)
swift test
```

`scripts/make-test-video.sh` and `Twin --replay` play test videos without a
phone; `scripts/fake-wda.py` stands in for WebDriverAgent.

## How it works

- Screen: CoreMediaIO's `kCMIOHardwarePropertyAllowScreenCaptureDevices` exposes the
  iPhone as a muxed `AVCaptureDevice`, drawn with `AVSampleBufferDisplayLayer`.
- Input: `imb-hid` (Rust, on [idevice](https://github.com/jkcoxson/idevice)) opens the
  CoreDevice tunnel over usbmuxd, holds a display media stream open (the phone ignores
  synthetic HID without one) and sends a touch report for every pointer move. These
  are private, reverse-engineered services, so an iOS update can break input.

## License

MIT
