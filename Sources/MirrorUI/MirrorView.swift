import AppKit
import AVFoundation
import IMBCore

/// Shows the phone's screen and turns mouse and keyboard input into touches.
/// While Cmd is held, a press moves the window and nothing reaches the phone.
final class MirrorView: NSView {
    let display = AVSampleBufferDisplayLayer()
    private let status = NSTextField(labelWithString: "")
    weak var input: InputSink?
    private var pressed = false
    private var commandPress = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
        display.videoGravity = .resizeAspectFill  // the window keeps the video's aspect; fill hides rounding slivers
        display.frame = bounds
        display.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
        layer?.addSublayer(display)


        status.textColor = .secondaryLabelColor
        status.alignment = .center
        status.maximumNumberOfLines = 0
        status.font = .systemFont(ofSize: 13)
        status.translatesAutoresizingMaskIntoConstraints = false
        addSubview(status)
        NSLayoutConstraint.activate([
            status.centerXAnchor.constraint(equalTo: centerXAnchor),
            status.centerYAnchor.constraint(equalTo: centerYAnchor),
            status.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, constant: -32),
        ])
    }

    required init?(coder: NSCoder) { nil }

    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var isFlipped: Bool { true }

    func setStatus(_ text: String?) {
        status.stringValue = text ?? ""
        status.isHidden = text == nil
    }

    /// The pointer from 0 to 1 across the phone's screen
    private func phonePoint(_ event: NSEvent) -> CGPoint {
        let p = convert(event.locationInWindow, from: nil)
        return CGPoint(x: min(max(p.x / max(bounds.width, 1), 0), 1),
                       y: min(max(p.y / max(bounds.height, 1), 0), 1))
    }

    /// A round pointer like iOS's own over the phone, darker while it touches
    static let touchCursor = circleCursor(fill: 0.42)
    static let pressedCursor = circleCursor(fill: 0.62)

    private static func circleCursor(fill: CGFloat) -> NSCursor {
        let size: CGFloat = 30
        let image = NSImage(size: NSSize(width: size, height: size), flipped: false) { rect in
            let circle = NSBezierPath(ovalIn: rect.insetBy(dx: 3, dy: 3))
            NSColor(white: 0.5, alpha: fill).setFill()
            circle.fill()
            NSColor(white: 1, alpha: 0.9).setStroke()
            circle.lineWidth = 2
            circle.stroke()
            // A faint dark rim keeps it visible on white
            let rim = NSBezierPath(ovalIn: rect.insetBy(dx: 1.5, dy: 1.5))
            NSColor(white: 0, alpha: 0.22).setStroke()
            rim.lineWidth = 1
            rim.stroke()
            return true
        }
        return NSCursor(image: image, hotSpot: NSPoint(x: size / 2, y: size / 2))
    }

    private var commandHeld = false

    private var currentCursor: NSCursor {
        if commandPress { return .closedHand }
        if commandHeld { return .openHand }
        return pressed ? Self.pressedCursor : Self.touchCursor
    }

    override func resetCursorRects() { addCursorRect(bounds, cursor: currentCursor) }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .activeAlways, .inVisibleRect, .cursorUpdate],
                                       owner: self))
    }

    override func cursorUpdate(with event: NSEvent) { currentCursor.set() }
    override func mouseMoved(with event: NSEvent) { setCommand(event.modifierFlags.contains(.command)) }
    override func flagsChanged(with event: NSEvent) { setCommand(event.modifierFlags.contains(.command)) }

    private func setCommand(_ held: Bool) {
        commandHeld = held
        window?.invalidateCursorRects(for: self)
        if let window, bounds.contains(convert(window.mouseLocationOutsideOfEventStream, from: nil)) {
            currentCursor.set()
        }
    }

    override func mouseDown(with event: NSEvent) {
        if event.modifierFlags.contains(.command) {
            commandPress = true
            currentCursor.set()
            window?.performDrag(with: event)
            commandPress = false
            setCommand(NSEvent.modifierFlags.contains(.command))
            return
        }
        commandPress = false
        pressed = true
        currentCursor.set()
        input?.touch(.down, at: phonePoint(event))
    }

    override func mouseDragged(with event: NSEvent) {
        guard pressed else { return }
        input?.touch(.move, at: phonePoint(event))
    }

    override func mouseUp(with event: NSEvent) {
        commandPress = false
        guard pressed else { return }
        pressed = false
        currentCursor.set()
        input?.touch(.up, at: phonePoint(event))
    }

    override func scrollWheel(with event: NSEvent) {
        guard !event.modifierFlags.contains(.command) else { return }
        // The finger moves with the content, as macOS already turned the deltas
        let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 10
        input?.scroll(at: phonePoint(event), by: CGVector(dx: event.scrollingDeltaX * scale / max(bounds.width, 1),
                                                          dy: event.scrollingDeltaY * scale / max(bounds.height, 1)))
    }

    override func keyDown(with event: NSEvent) {
        guard !event.modifierFlags.contains(.command), !event.modifierFlags.contains(.control) else {
            super.keyDown(with: event)
            return
        }
        switch event.keyCode {
        case 51: input?.type("\u{8}")    // delete
        case 117: input?.type("\u{7F}")  // forward delete
        case 36, 76: input?.type("\n")   // return, enter
        case 48: input?.type("\t")
        case 53: input?.key(0x29)   // escape
        case 123: input?.key(0x50)  // arrows: left, right, down, up
        case 124: input?.key(0x4F)
        case 125: input?.key(0x51)
        case 126: input?.key(0x52)
        default:
            if let text = event.characters, !text.isEmpty,
               text.unicodeScalars.allSatisfy({ $0.value >= 0x20 && !(0xF700...0xF8FF).contains($0.value) }) {
                input?.type(text)
            }
        }
    }
}
