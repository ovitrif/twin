import AppKit
import Testing
@testable import MirrorUI

/// A 300 × 600 mirror at (100, 100) with its ring, never shown
@MainActor private func makeRing() -> (NSWindow, ResizeRing) {
    let mirror = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 300, height: 600),
                          styleMask: [.borderless], backing: .buffered, defer: true)
    let ring = ResizeRing(mirror: mirror)
    ring.update(radius: 42, shown: true)
    return (mirror, ring)
}

/// A mouse event at a point in the ring window's coordinates
@MainActor private func mouse(_ type: NSEvent.EventType, _ p: NSPoint, _ ring: ResizeRing) -> NSEvent {
    NSEvent.mouseEvent(with: type, location: p, modifierFlags: [], timestamp: 0, windowNumber: ring.panel.windowNumber,
                       context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
}

@MainActor @Test func theRingHugsTheOutlineAndLeavesTheScreenToThePhone() {
    let (_, ring) = makeRing()
    let view = ring.view
    // The ring is 6 pt wider on each side: (0, 0) to (312, 612)
    #expect(view.hitTest(NSPoint(x: 3, y: 306)) === view)    // just left of the left edge
    #expect(view.hitTest(NSPoint(x: 156, y: 306)) == nil)    // on the phone's screen
    #expect(view.hitTest(NSPoint(x: 2, y: 2)) == nil)        // the empty corner outside the curve
    #expect(view.hitTest(NSPoint(x: 16, y: 16)) === view)    // on the curve itself
    #expect(view.edge(at: NSPoint(x: 3, y: 306)) == Edge(x: -1, y: 0))
    #expect(view.edge(at: NSPoint(x: 156, y: 609)) == Edge(x: 0, y: 1))
    #expect(view.edge(at: NSPoint(x: 16, y: 16)) == Edge(x: -1, y: -1))
}

@MainActor @Test func draggingAnEdgeKeepsThePhonesShape() {
    let (mirror, ring) = makeRing()
    let view = ring.view
    // Pull the right edge out by 60 pt: 360 wide, so 720 tall, top kept
    view.mouseDown(with: mouse(.leftMouseDown, NSPoint(x: 309, y: 306), ring))
    view.mouseDragged(with: mouse(.leftMouseDragged, NSPoint(x: 369, y: 306), ring))
    #expect(mirror.frame == NSRect(x: 100, y: -20, width: 360, height: 720))
    view.mouseUp(with: mouse(.leftMouseUp, NSPoint(x: 369, y: 306), ring))
}

@MainActor @Test func draggingTheBottomLeftCornerKeepsTheTopRight() {
    let (mirror, ring) = makeRing()
    let view = ring.view
    // Pull the corner in by 30 pt down and left: 660 tall, 330 wide
    view.mouseDown(with: mouse(.leftMouseDown, NSPoint(x: 16, y: 16), ring))
    view.mouseDragged(with: mouse(.leftMouseDragged, NSPoint(x: -14, y: -44), ring))
    #expect(mirror.frame.maxX == 400 && mirror.frame.maxY == 700)
    #expect(mirror.frame.size == NSSize(width: 330, height: 660))
}
