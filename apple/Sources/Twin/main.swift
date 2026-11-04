import AppKit
import MirrorUI

// Twin Mirror: one app for every mirror dmb opens, so macOS asks camera access
// once. With an Android phone (--serial) it shows that phone, beside the
// iPhone when --udid names one; otherwise just the iPhone. Opened on its own (Finder,
// Spotlight) it quits, since only dmb knows which phones to show.

let args = Array(CommandLine.arguments.dropFirst())
let app = NSApplication.shared
let pair = PairOptions(args)
let single = Options(args)
if pair.isEmpty && single.isEmpty {
    FileHandle.standardError.write(Data("Twin shows the phones `dmb mirror` or `dmb devices` asks for.\n".utf8))
    exit(0)
}
app.setActivationPolicy(.regular)
let delegate: NSApplicationDelegate = pair.isEmpty ? AppDelegate(options: single) : PairDelegate(options: pair)
app.delegate = delegate
app.run()
