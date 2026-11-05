# Repository Instructions

## Support
- Twin supports Apple silicon Macs on current macOS, and the `twin` CLI on Linux
  (Android only). No Intel Macs and no workarounds for outdated systems: build,
  test and release for arm64 macOS only.

## Workflow
- After every separable unit of work, commit the completed changes directly.
- Keep commits scoped to the completed unit of work.
- Before committing Rust changes, run:
  - `cargo fmt --all -- --check`
  - `cargo test`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo build --release`
- Before committing changes under `apple/`, run `swift test` there and `scripts/bundle-app.sh`.

## Patch Release Shipping
When Ovi asks to ship a patch release:
1. Create a dedicated `ovi/` release branch and pull request containing the complete patch diff, version bump, install documentation, and release notes context.
2. Keep the pull request in review until required local/product validation and CI pass, actionable feedback is fixed, addressed threads are resolved, and the unchanged final head receives two clean fresh-context reviews.
3. Immediately before merging, record and reverify the exact reviewed pull-request head, passing checks, and resolved feedback. Merge with a merge commit, then verify that the resulting merge commit contains that reviewed head as its pull-request parent. Do not use squash or rebase merge.
4. Create and push the annotated `vX.Y.Z` tag at the exact merge commit, verify the remote tag resolves to that commit, then wait for the tag-triggered Release workflow. Verify the GitHub release, all expected platform archives, `install.sh`, and `checksums.sha256`, and confirm GitHub associates the stable tag with the merged pull request. Curate the release notes to the established `This release:` bullet format when generated notes do not match it.

## Reinstall after every CLI/app change (required)
Any change to `crates/twin` (the `twin` CLI), `crates/twin-app` (the menu bar
app) or `apple/` (Twin Mirror, dmb, dmb-hid) MUST be followed by a rebuild,
reinstall, and restart so the installed tools match the source and the running
menu bar app is not left stale:
- `cargo build --release`
- `install -m 755 target/release/twin /Users/ovitrif/.local/bin/twin` and `ln -sf twin /Users/ovitrif/.local/bin/awb`
- Rebundle the app: `cargo build --release -p twin-app` first (the bundle script builds only Twin Mirror), then `scripts/bundle-app.sh`, then replace `/Applications/Twin.app` by moving `target/bundle/Twin.app` there (move, not copy: a bundle left in `target/` shows up in Spotlight as a second app).
- Replace the running instance (overwriting the binary does not update an already-running process): quit it (`osascript -e 'quit app "Twin"'`), then relaunch with `open -g /Applications/Twin.app`. Quitting closes open mirrors.
- Verify: `/Users/ovitrif/.local/bin/twin --version`.

## Layout and spacing
- The menu bar app follows the Twin redesign (popover 420 pt wide, grouped cards):
  building blocks live in `crates/twin-app/src/kit.rs` (cards, 36 pt icon buttons with
  20 pt icons, toggles, the segmented control, the device glyphs drawn as vectors).
- Space it on the kit's grid: 20 pt popover padding, 16 pt between sections and
  between phone rows, 8 pt from a heading to its card; card rows inset 16 pt left and
  12 pt right; list rows 52 pt, setting rows 44 pt.
- Section headings ("Physical", "Virtual", the Settings groups) are `kit::heading`,
  never a one-off label style. Rows carry no status text: a button's look says it.

## Verify the app UI
Check every UI change yourself before calling it done, without the user's
screen, mouse or keyboard: drive the real popover headlessly.

- Build and start the drive server (renders offscreen; no window, no menu bar
  item, no focus stealing) in its own target folder, so the drive build never
  replaces `target/release/twin-app` (a normal build run with `--drive` starts
  a real second menu bar app):
  `CARGO_TARGET_DIR=~/.local/state/twin/drive-target cargo build --release -p twin-app --features drive`,
  then `~/.local/state/twin/drive-target/release/twin-app --drive /tmp/twind.sock &`. Keep the socket path
  short (Unix sockets cap it near 100 bytes) and end the server with
  `~/.local/state/twin/drive-target/release/twin-app drive /tmp/twind.sock quit`
  when done (the installed `twin-app` lacks the drive feature).
- Send commands with `~/.local/state/twin/drive-target/release/twin-app drive /tmp/twind.sock <command>`,
  or pipe one per line with `-`. Each reply is one `ok ...` line carrying the
  current `screen`, `tab`, `transition`, `theme`, `gradients` and `pairing`
  phase. Commands: `state`, `click X Y`, `hover X Y`, `drag X1 Y1 X2 Y2`, `leave`, `scroll DY`,
  `key NAME` (egui names, `shift+Tab`), `wait MS`, `shot PATH`,
  `record DIR` / `stop` (every frame plus `times.txt`), `theme auto|day|night`,
  `gradients on|off`, `window on|off` (the detached window, 860 × 600, drawn in
  the same frame), `quit`. Coordinates are window points: 420 wide, 0 at the top
  of the beak; Refresh sits at (382, 48) and the tabs at y 97, x 68 (Devices),
  162 (Pair), 257 (Logs) and 351 (Settings); in the window the sidebar pages sit
  at x 100, y 110, 150, 190 and 230. Send `leave` before a `shot` meant for
  sharing, so no pointer is drawn. The full reference is in
  `crates/twin-app/src/drive.rs`.
- The drive server isolates settings only: it uses a throwaway config folder
  (seed it with `TWIN_DRIVE_CONFIG=<config.toml>`) and never touches the login
  item. Everything else is real: adb, the emulator, `avdmanager` and scrcpy,
  so confirming a delete removes a real AVD and Play starts a real emulator or
  mirror. Run it with demo AVDs (below) unless real ones are the point, and
  before clicking any Play put a stub for `scrcpy` (passing `--version` and
  `--help` through to the real one) first on `PATH` and point `TWIN_DMB` at a
  `dmb` stub (Twin finds the installed app's dmb before one on `PATH`) that
  answers `status --json` with `{"devices": []}`, so no mirror opens on the
  user's screen.
- Demo devices (drive builds only): `TWIN_DEMO_DEVICES=1` lists a Wi-Fi Pixel,
  a USB iPhone, a running emulator (an AVD named `Medium_Phone`) and a
  simulator, for captures without real phones.
- Light and dark mode: `theme day` / `theme night` / `theme auto` over the
  socket. In the running app, use the Appearance control in Settings or set
  `theme = "day"`, `"night"` or `"auto"` in `~/.config/twin/config.toml`
  (honors `XDG_CONFIG_HOME`), then restart it. Check both themes, and both
  `gradients on` and `off`, for every visual change.
- Pairing without a phone (drive builds only): `TWIN_MOCK_PAIRING=success` or `failure`, with an
  optional `:seconds` before the simulated scan (`success:1.5`), runs the real
  pairing screens with a simulated scan; success lists `Pixel 9 Pro (mock)`.
- Emulators without touching real AVDs: point `ANDROID_AVD_HOME` at a folder
  of `<Name>.ini` files whose `path=` names an existing `<Name>.avd` folder.
- Before publishing a capture, keep private names out of it: use demo AVDs,
  and put `scutil` and `hostname` stubs that print `twin-demo` first on `PATH`,
  since the pairing QR embeds the machine name.
- Cover every screen and state a change can reach: Devices (checking, empty,
  phones and emulators, a scrolled list with its edge fades and scrollbar),
  Logs, Settings, Pair (QR, connecting, failed, paired), the delete-emulator
  dialog, Tab focus highlights, ← / → page navigation, the detached window
  (`window on`), and the slide of every transition. Moving right in the tabs
  (Devices → Pair → Logs → Settings) slides left, moving left slides right, both
  pages stay fully drawn throughout and the tab's pill slides with them;
  `record` and read the frames to confirm.

## Verify the mirror windows
- Check Twin Mirror changes without the user's screen: run it headless in the
  `aivm` VM with `--replay` test videos (`apple/scripts/make-test-video.sh`), or
  `--android-replay` and `--iphone-replay` for the side by side window, and
  capture its windows with `screencapture -l <window id>` (wake the VM's
  display with `caffeinate -u` first); `DMB_SHOW_BAR=1` keeps the title bar
  shown for those captures.
- `dmb-hid` talks to private CoreDevice services pinned to one idevice commit;
  test input changes on a real iPhone on iOS 27+.
- `dmb-hid --ui` (behind `twin iphone ui`) reads the accessibility audit daemon
  (`com.apple.accessibility.axAuditDaemon.remoteserver.shim.remote`, DTX) in
  `apple/hid/src/ui.rs`. The shim needs an RSD check-in before DTX; read its
  two opening messages before sending; setters take plain values, attribute
  reads take the attribute objects from the element's own event; after a
  session's cleanup the inspector cursor is parked and "first" does nothing
  until "next" moves it. Element frames, point lookups and pressing are not
  available (pressing needs a development-signed app). Never walk or press
  inside a wallet holding real funds on a test phone; read-only walks only.

## Layout
- `crates/twin-core`: shared ADB/scrcpy/QR/mDNS logic, and finding Twin.app's helpers (lib).
- `crates/twin`: the `twin` CLI; `twin iphone` runs the bundled dmb.
- `crates/twin-app`: the macOS menu bar app (`twin-app`), design source in `DESIGN.pen`.
- `apple/`: Twin Mirror (the iPhone and side by side windows, Swift), the `dmb` helper CLI and `apple/hid` (`dmb-hid`, Rust, outside the workspace).
- `scripts/bundle-app.sh` builds `target/bundle/Twin.app`: the menu bar app, with `Twin Mirror.app` (every mirror window) and Google's `adb` under `Contents/Helpers`.

## CI / GitHub Actions
- GitHub Action workflow file changes only take effect on PRs opened after the merge of the PR that modifies them. Always note "(after merge)" in test plan items about verifying workflow behavior.
