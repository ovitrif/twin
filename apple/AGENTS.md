# Repository Instructions

- Commit each finished unit of work; before committing run `swift test` and `scripts/bundle-app.sh`.
- After every change to the CLI, the app or `dmb-hid`, reinstall so the installed tools match the source:
  - `scripts/bundle-app.sh`
  - `install -m 755 build/dmb ~/.local/bin/dmb`
  - replace `/Applications/Twin.app` by moving `build/Twin.app` there (move, not copy: a bundle left in `build/` shows up in Spotlight as a second app)
- Check UI changes without the user's screen: run the app headless in the `aivm` VM with `--replay` test videos (`scripts/make-test-video.sh`) and capture its windows with `screencapture -l <window id>`; `DMB_SHOW_BAR=1` keeps the title bar shown for those captures.
- `dmb-hid` talks to private CoreDevice services pinned to one idevice commit; test input changes on a real iPhone on iOS 27+.
