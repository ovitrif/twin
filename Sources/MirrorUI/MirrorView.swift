import AppKit
import AVFoundation
import IMBCore

/// Shows the phone's screen and turns mouse and keyboard input into touches.
/// While Cmd is held, a press moves the window and nothing reaches the phone.
final class MirrorView: NSView {
    let display = AVSampleBufferDisplayLayer()
    private let status = NSTextField(labelWithString: "")
    private let touchDot = CAShapeLayer()
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

        touchDot.fillColor = NSColor.white.withAlphaComponent(0.35).cgColor
        touchDot.strokeColor = NSColor.white.withAlphaComponent(0.8).cgColor
        touchDot.lineWidth = 1.5
        touchDot.path = CGPath(ellipseIn: CGRect(x: -14, y: -14, width: 28, height: 28), transform: nil)
        touchDot.isHidden = true
        layer?.addSublayer(touchDot)

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

    private func showDot(at event: NSEvent?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if let event {
            touchDot.position = convert(event.locationInWindow, from: nil)
            touchDot.isHidden = false
        } else {
            touchDot.isHidden = true
        }
        CATransaction.commit()
    }

    override func mouseDown(with event: NSEvent) {
        if event.modifierFlags.contains(.command) {
            commandPress = true
            window?.performDrag(with: event)
            return
        }
        commandPress = false
        pressed = true
        input?.touch(.down, at: phonePoint(event))
        showDot(at: event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard pressed else { return }
        input?.touch(.move, at: phonePoint(event))
        showDot(at: event)
    }

    override func mouseUp(with event: NSEvent) {
        showDot(at: nil)
        commandPress = false
        guard pressed else { return }
        pressed = false
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
