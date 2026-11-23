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

@Test func barBridgeKeepsBarUpOverGapAndRoundedCornerUnlessCovered() {
    let parent = NSRect(x: 100, y: 100, width: 440, height: 980)
    let bar = NSRect(x: 100, y: parent.maxY + 6, width: 440, height: 28)
    let corner: CGFloat = 53
    let mirrorWin = 10
    let ringWin = 11
    let behindWin = 20
    let coveringWin = 30
    let own: (Int) -> Bool = { $0 == mirrorWin || $0 == ringWin }
    let above: (Int) -> Bool = { $0 == coveringWin }

    // 6 pt gap over desktop, behind-mirror window, or our own resize ring keeps the bar up
    for top in [0, behindWin, ringWin] {
        #expect(WindowStyler.overBarBridge(
            mouse: NSPoint(x: 116, y: parent.maxY + 3),
            barFrame: bar, parentFrame: parent, cornerRadius: corner,
            top: top, mirrorWindowNumber: mirrorWin, isOwnWindow: own, isAboveMirror: above
        ))
    }

    // 6 pt gap covered by a window above the mirror drops the bar
    #expect(!WindowStyler.overBarBridge(
        mouse: NSPoint(x: 300, y: parent.maxY + 3),
        barFrame: bar, parentFrame: parent, cornerRadius: corner,
        top: coveringWin, mirrorWindowNumber: mirrorWin, isOwnWindow: own, isAboveMirror: above
    ))

    // Transparent rounded-corner cutaway right below the traffic lights keeps the bar up when uncovered
    #expect(WindowStyler.overBarBridge(
        mouse: NSPoint(x: 116, y: parent.maxY - 12),
        barFrame: bar, parentFrame: parent, cornerRadius: corner,
        top: behindWin, mirrorWindowNumber: mirrorWin, isOwnWindow: own, isAboveMirror: above
    ))

    // Covered rounded-corner cutaway drops the bar
    #expect(!WindowStyler.overBarBridge(
        mouse: NSPoint(x: 116, y: parent.maxY - 12),
        barFrame: bar, parentFrame: parent, cornerRadius: corner,
        top: coveringWin, mirrorWindowNumber: mirrorWin, isOwnWindow: own, isAboveMirror: above
    ))

    // Opaque phone pixels inside the top strip do not keep the bar stuck open after ⌘ is released
    #expect(!WindowStyler.overBarBridge(
        mouse: NSPoint(x: 300, y: parent.maxY - 12),
        barFrame: bar, parentFrame: parent, cornerRadius: corner,
        top: mirrorWin, mirrorWindowNumber: mirrorWin, isOwnWindow: own, isAboveMirror: above
    ))

    // Flipped inside at the top screen edge: only the gap directly under the bar bridges
    let insideBar = NSRect(x: 100, y: parent.maxY - 28 - 6, width: 440, height: 28)
    #expect(WindowStyler.overBarBridge(
        mouse: NSPoint(x: 116, y: insideBar.minY - 3),
        barFrame: insideBar, parentFrame: parent, cornerRadius: corner,
        top: mirrorWin, mirrorWindowNumber: mirrorWin, isOwnWindow: own, isAboveMirror: above
    ))
    #expect(!WindowStyler.overBarBridge(
        mouse: NSPoint(x: 116, y: parent.maxY - 2),
        barFrame: insideBar, parentFrame: parent, cornerRadius: corner,
        top: behindWin, mirrorWindowNumber: mirrorWin, isOwnWindow: own, isAboveMirror: above
    ))
}
