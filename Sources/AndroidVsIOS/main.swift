import AppKit
import MirrorUI

let options = PairOptions(Array(CommandLine.arguments.dropFirst()))
if options.isEmpty {
    FileHandle.standardError.write(Data("Android v iOS shows the phones awb's Side by side opens; run `imb devices`.\n".utf8))
    exit(0)
}
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = PairDelegate(options: options)
app.delegate = delegate
app.run()
