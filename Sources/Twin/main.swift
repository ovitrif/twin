import AppKit
import MirrorUI

// Twin: one app for every mirror imb opens, so macOS asks camera access once.
// With an Android phone (--serial) it shows that phone and the iPhone side by
// side in one window; otherwise just the iPhone. Opened on its own (Finder,
// Spotlight) it quits, since only imb knows which phones to show.

let args = Array(CommandLine.arguments.dropFirst())
let app = NSApplication.shared
let pair = PairOptions(args)
let single = Options(args)
if pair.isEmpty && single.isEmpty {
    FileHandle.standardError.write(Data("Twin shows the phones `imb mirror` or `imb devices` asks for.\n".utf8))
    exit(0)
}
app.setActivationPolicy(.regular)
let delegate: NSApplicationDelegate = pair.isEmpty ? AppDelegate(options: single) : PairDelegate(options: pair)
app.delegate = delegate
app.run()
