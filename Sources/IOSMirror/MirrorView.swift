import AppKit
import AVFoundation
import IMBCore

/// Shows the phone's screen and turns mouse and keyboard input into touches.
/// While Cmd is held, a press moves the window and nothing reaches the phone.
final class MirrorView: NSView {
    let display = AVSampleBufferDisplayLayer()
    private let status = NSTextField(labelWithString: "")
    private let touchDot = CAShapeLayer()
    var onGesture: ((Gesture) -> Void)?
    var onScroll: ((CGPoint, CGVector) -> Void)?
    var onKeys: ((String) -> Void)?
    /// The phone's screen in points, for mapping the pointer; until known, the video's size.
    var deviceSize = CGSize(width: 1, height: 1)

    private var pressStart: TimeInterval?
    private var points: [CGPoint] = []
    private var times: [TimeInterval] = []
    private var commandPress = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
        display.videoGravity = .resizeAspect
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

    private func phonePoint(_ event: NSEvent) -> CGPoint {
        Screen.devicePoint(convert(event.locationInWindow, from: nil), view: bounds.size, device: deviceSize)
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
        pressStart = event.timestamp
        points = [phonePoint(event)]
        times = [0]
        showDot(at: event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard !commandPress, let start = pressStart else { return }
        points.append(phonePoint(event))
        times.append(event.timestamp - start)
        showDot(at: event)
    }

    override func mouseUp(with event: NSEvent) {
        showDot(at: nil)
        guard !commandPress, let start = pressStart else { commandPress = false; return }
        pressStart = nil
        let point = phonePoint(event)
        if point != points.last {
            points.append(point)
        } else {
            points.removeLast()
            times.removeLast()
            points.append(point)
        }
        times.append(event.timestamp - start)
        onGesture?(Gesture(points: points, times: times))
    }

    override func scrollWheel(with event: NSEvent) {
        guard !event.modifierFlags.contains(.command) else { return }
        let scale = deviceSize.width / max(bounds.width, 1)
        onScroll?(phonePoint(event), CGVector(dx: event.scrollingDeltaX * scale, dy: event.scrollingDeltaY * scale))
    }

    override func keyDown(with event: NSEvent) {
        guard !event.modifierFlags.contains(.command), !event.modifierFlags.contains(.control) else {
            super.keyDown(with: event)
            return
        }
        switch event.keyCode {
        case 51: onKeys?("\u{8}")    // delete
        case 117: onKeys?("\u{7F}")  // forward delete
        case 36, 76: onKeys?("\n")   // return, enter
        case 48: onKeys?("\t")
        default:
            if let text = event.characters, !text.isEmpty,
               text.unicodeScalars.allSatisfy({ $0.value >= 0x20 && !(0xF700...0xF8FF).contains($0.value) }) {
                onKeys?(text)
            }
        }
    }
}
