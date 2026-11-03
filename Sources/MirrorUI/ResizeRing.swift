import AppKit

// Resizing from the visible outline. macOS puts the resize zones of a
// borderless window on its rectangle, a few points outside it, so with the
// phone's rounded corners they sit away from what you see. The ring is a
// child window around the mirror that takes the pointer only in a thin band
// hugging the rounded outline, shows the matching resize cursor and resizes
// the mirror keeping the phone's aspect ratio.

private let band: CGFloat = 6
private let minimumHeight: CGFloat = 320

/// Which sides a drag moves: -1 the left or bottom side, 1 the right or top, 0 neither
struct Edge: Equatable {
    var x: Int
    var y: Int
}

final class RingView: NSView {
    weak var mirror: NSWindow?
    var radius: CGFloat = 0
    private var dragEdge: Edge?
    private var startFrame = NSRect.zero
    private var startMouse = NSPoint.zero

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    /// The band between the mirror's outline and the same outline `band` further out
    private var ringPath: NSBezierPath {
        let inner = bounds.insetBy(dx: band, dy: band)
        let path = NSBezierPath(roundedRect: bounds, xRadius: radius + band, yRadius: radius + band)
        path.append(NSBezierPath(roundedRect: inner, xRadius: radius, yRadius: radius))
        path.windingRule = .evenOdd
        return path
    }

    override func draw(_ dirtyRect: NSRect) {
        // Nearly invisible, but not transparent: the window server passes
        // clicks through fully transparent pixels
        NSColor(white: 0, alpha: 0.012).setFill()
        ringPath.fill()
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        ringPath.contains(convert(point, from: superview)) ? self : nil
    }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .cursorUpdate],
                                       owner: self))
    }

    func edge(at point: NSPoint) -> Edge {
        let inner = bounds.insetBy(dx: band, dy: band)
        // Within the corner's curve both sides move
        let reach = max(radius, 12)
        let x = point.x < inner.minX + reach ? -1 : (point.x > inner.maxX - reach ? 1 : 0)
        let y = point.y < inner.minY + reach ? -1 : (point.y > inner.maxY - reach ? 1 : 0)
        if x != 0 && y != 0 {
            return Edge(x: x, y: y)
        }
        // Along a side, the side the point is outside of
        if point.x < inner.minX { return Edge(x: -1, y: 0) }
        if point.x > inner.maxX { return Edge(x: 1, y: 0) }
        return Edge(x: 0, y: point.y < inner.minY ? -1 : 1)
    }

    private func cursor(for edge: Edge) -> NSCursor {
        if #available(macOS 15.0, *) {
            let position: NSCursor.FrameResizePosition
            switch (edge.x, edge.y) {
            case (-1, 1): position = .topLeft
            case (1, 1): position = .topRight
            case (-1, -1): position = .bottomLeft
            case (1, -1): position = .bottomRight
            case (-1, 0): position = .left
            case (1, 0): position = .right
            case (0, 1): position = .top
            default: position = .bottom
            }
            return .frameResize(position: position, directions: .all)
        }
        return edge.y == 0 ? .resizeLeftRight : .resizeUpDown
    }

    override func mouseMoved(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        if ringPath.contains(point) { cursor(for: edge(at: point)).set() }
    }

    override func cursorUpdate(with event: NSEvent) { mouseMoved(with: event) }

    override func mouseExited(with event: NSEvent) {
        if dragEdge == nil { NSCursor.arrow.set() }
    }

    override func mouseDown(with event: NSEvent) {
        guard let mirror else { return }
        dragEdge = edge(at: convert(event.locationInWindow, from: nil))
        startFrame = mirror.frame
        startMouse = screenPoint(event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let mirror, let edge = dragEdge, startFrame.height > 0 else { return }
        cursor(for: edge).set()
        let aspect = startFrame.width / startFrame.height
        let mouse = screenPoint(event)
        let dx = (mouse.x - startMouse.x) * CGFloat(edge.x)
        let dy = (mouse.y - startMouse.y) * CGFloat(edge.y)
        // The side the pointer pulls furthest sets the size; the other follows the aspect
        var height = startFrame.height + max(dy, dx / aspect)
        if edge.x == 0 { height = startFrame.height + dy }
        if edge.y == 0 { height = startFrame.height + dx / aspect }
        let screen = mirror.screen?.visibleFrame.height ?? .greatestFiniteMagnitude
        height = min(max(height, minimumHeight), screen).rounded()
        let width = (height * aspect).rounded()
        // The opposite sides stay put; a side drag keeps the top, a top or
        // bottom drag keeps the left
        let x = edge.x == -1 ? startFrame.maxX - width : startFrame.minX
        let y = edge.y == -1 ? startFrame.maxY - height : (edge.y == 1 ? startFrame.minY : startFrame.maxY - height)
        mirror.setFrame(NSRect(x: x, y: y, width: width, height: height), display: true)
    }

    override func mouseUp(with event: NSEvent) { dragEdge = nil }

    private func screenPoint(_ event: NSEvent) -> NSPoint {
        window?.convertPoint(toScreen: event.locationInWindow) ?? NSEvent.mouseLocation
    }
}

final class ResizeRing {
    private let mirror: NSWindow
    let panel: NSPanel
    let view = RingView()

    init(mirror: NSWindow) {
        self.mirror = mirror
        panel = NSPanel(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.hidesOnDeactivate = false
        panel.becomesKeyOnlyIfNeeded = true
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.transient, .ignoresCycle]
        panel.acceptsMouseMovedEvents = true
        panel.setAccessibilityElement(false)
        view.mirror = mirror
        panel.contentView = view
    }

    /// Fits the ring around the mirror; hidden while the mirror is full screen
    func update(radius: CGFloat, shown: Bool) {
        guard shown else {
            if panel.parent != nil { mirror.removeChildWindow(panel) }
            panel.orderOut(nil)
            return
        }
        view.radius = radius
        panel.setFrame(mirror.frame.insetBy(dx: -band, dy: -band), display: false)
        view.needsDisplay = true
        if panel.parent == nil {
            mirror.addChildWindow(panel, ordered: .below)
        }
        panel.level = mirror.level
    }
}
