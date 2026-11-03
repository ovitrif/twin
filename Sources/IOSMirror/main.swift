import AppKit
import MirrorUI

let options = Options(Array(CommandLine.arguments.dropFirst()))
if options.isEmpty {
    FileHandle.standardError.write(Data("iOS Mirror shows the phone `imb mirror` asks for; run `imb mirror`.\n".utf8))
    exit(0)
}
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = AppDelegate(options: options)
app.delegate = delegate
app.run()
