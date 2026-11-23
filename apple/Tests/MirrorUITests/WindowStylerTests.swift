import AppKit
import Testing
@testable import MirrorUI

@MainActor @Test func aWindowWithNoPanesLeavesTheBarAlone() {
    let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 300, height: 600),
                          styleMask: [.borderless], backing: .buffered, defer: true)
    window.contentView = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 600))
    let styler = WindowStyler(window: window, title: "Pair", cornerRatio: 0.1)
    styler.panes = { [] }  // both phones hidden, the moment before the window closes
    styler.layoutBar()
    styler.applyStyle()
}
