import AppKit
import AVFoundation
import DMBCore

/// Tracks decode-failure recovery for a video layer: after a failure or a
/// required flush, non-sync P-frames are dropped until a keyframe arrives so
/// they cannot immediately re-fail the decoder; if recovery stalls past
/// `stallTimeout`, the caller is asked to restart the session.
struct DecodeRecovery {
    struct Action: Equatable {
        var flush = false
        var enqueue = true
        var requestKeyframe = false
        var logError: String?
        var restartSession = false
    }

    let stallTimeout: TimeInterval
    private(set) var waitingForKeyframe = false
    private(set) var stuckSince: TimeInterval?

    init(stallTimeout: TimeInterval = 5) {
        self.stallTimeout = stallTimeout
    }

    mutating func reset() {
        waitingForKeyframe = false
        stuckSince = nil
    }

    mutating func step(isSync: Bool, needsFlush: Bool, error: String?, now: TimeInterval) -> Action {
        if needsFlush {
            var action = Action(flush: true, enqueue: false, requestKeyframe: !isSync)
            if stuckSince == nil {
                stuckSince = now
                action.logError = error ?? "decode failed"
            }
            if isSync {
                // Keep `stuckSince` until the next frame proves this keyframe decoded cleanly
                waitingForKeyframe = false
                action.enqueue = true
            } else {
                waitingForKeyframe = true
                if let since = stuckSince, now - since >= stallTimeout {
                    action.restartSession = true
                    stuckSince = now
                }
            }
            return action
        }

        if waitingForKeyframe {
            if isSync {
                waitingForKeyframe = false
                return Action(flush: false, enqueue: true)
            }
            var action = Action(flush: false, enqueue: false, requestKeyframe: true)
            if let since = stuckSince, now - since >= stallTimeout {
                action.restartSession = true
                stuckSince = now
            }
            return action
        }

        stuckSince = nil
        return Action(flush: false, enqueue: true)
    }

    /// Fires if recovery is still waiting for a keyframe when frames stop arriving
    mutating func checkWatchdog(now: TimeInterval) -> Bool {
        guard waitingForKeyframe, let since = stuckSince, now - since >= stallTimeout else { return false }
        stuckSince = now
        return true
    }
}

/// Shows the phone's screen and turns mouse and keyboard input into touches.
/// While Cmd is held, a press moves the window and nothing reaches the phone.
final class MirrorView: NSView {
    let display = AVSampleBufferDisplayLayer()
    private let status = NSTextField(labelWithString: "")
    weak var input: InputSink?
    private var pressed = false
    private var commandPress = false
    private let decodeLock = NSLock()
    private var decodeRecovery = DecodeRecovery()
    private var stallTimer: Timer?

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

    /// Rounds this phone in a window of several
    var cornerRadius: CGFloat = 0 {
        didSet {
            layer?.cornerRadius = cornerRadius
            layer?.cornerCurve = .continuous
            layer?.masksToBounds = cornerRadius > 0
        }
    }

    /// The screen's size when the video pads it (the Device Hub stream pads
    /// to a multiple of 16 at the right and bottom); the padding is cropped
    var screenSize: CGSize? {
        get { sizes.withLock { screen } }
        set {
            sizes.withLock { screen = newValue }
            needsLayout = true
        }
    }
    /// Frames come on a background queue while the main thread sets the
    /// screen's size, so the sizes below sit behind this lock
    private let sizes = NSLock()
    private var screen: CGSize?
    private var coded = CGSize.zero
    private var waits = false

    /// For a stream that pads the screen: until `screenSize` arrives, frames
    /// show but never size the window to their padded size
    var waitsForScreenSize: Bool {
        get { sizes.withLock { waits } }
        set { sizes.withLock { waits = newValue } }
    }

    /// The decoder failed and was reset; the stream should send a key frame.
    /// Runs on the frame's queue.
    var onDecodeFailure: (() -> Void)?
    /// First error observed when entering a decode-failure episode. Runs on any queue.
    var onDecodeError: ((String) -> Void)?
    /// Decode stayed stuck waiting for a keyframe past the stall timeout; the
    /// session should be restarted. Runs on any queue.
    var onDecodeStall: (() -> Void)?

    /// Whether `buffer` can start decoding right after a flush (a keyframe or uncompressed frame)
    static func isSyncFrame(_ buffer: CMSampleBuffer) -> Bool {
        guard let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false),
              CFArrayGetCount(attachments) > 0 else { return true }
        let dict = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFDictionary.self)
        guard let value = CFDictionaryGetValue(dict, Unmanaged.passUnretained(kCMSampleAttachmentKey_NotSync).toOpaque())
        else { return true }
        return !CFEqual(unsafeBitCast(value, to: CFTypeRef.self), kCFBooleanTrue)
    }

    private func armStallWatchdog() {
        DispatchQueue.main.async { [weak self] in
            guard let self, self.stallTimer == nil else { return }
            let timeout = self.decodeLock.withLock { self.decodeRecovery.stallTimeout }
            self.stallTimer = Timer.scheduledTimer(withTimeInterval: timeout, repeats: false) { [weak self] _ in
                guard let self else { return }
                self.stallTimer = nil
                let stalled = self.decodeLock.withLock {
                    self.decodeRecovery.checkWatchdog(now: ProcessInfo.processInfo.systemUptime)
                }
                if stalled { self.onDecodeStall?() }
            }
        }
    }

    private func disarmStallWatchdog() {
        DispatchQueue.main.async { [weak self] in
            self?.stallTimer?.invalidate()
            self?.stallTimer = nil
        }
    }

    override func layout() {
        super.layout()
        let (screenSize, codedSize) = sizes.withLock { (screen, coded) }
        guard var screen = screenSize, codedSize.width > 0, screen.width > 0 else {
            display.frame = bounds
            return
        }
        // The stream turns with the phone; the screenshot size does not
        if (codedSize.width > codedSize.height) != (screen.width > screen.height) {
            screen = CGSize(width: screen.height, height: screen.width)
        }
        let width = bounds.width * codedSize.width / screen.width
        let height = bounds.height * codedSize.height / screen.height
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        display.frame = CGRect(x: 0, y: display.isGeometryFlipped || layer?.isGeometryFlipped == true ? 0 : bounds.height - height,
                               width: width, height: height)
        CATransaction.commit()
    }

    /// Shows a frame as soon as it arrives and returns the size the window
    /// should take: the screen's, without any padding; nil while that is
    /// not known yet or while waiting for a keyframe after a decode error.
    /// Runs on any queue.
    @discardableResult
    func enqueue(_ buffer: CMSampleBuffer) -> CGSize? {
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: true),
           CFArrayGetCount(attachments) > 0 {
            let dict = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(dict, Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
                                 Unmanaged.passUnretained(kCFBooleanTrue).toOpaque())
        }
        let renderer = display.sampleBufferRenderer
        let isSync = Self.isSyncFrame(buffer)
        let needsFlush = renderer.status == .failed || renderer.requiresFlushToResumeDecoding
        let errText = renderer.error?.localizedDescription
            ?? (renderer.requiresFlushToResumeDecoding ? "renderer requires flush" : nil)
        let (action, stillWaiting) = decodeLock.withLock { () -> (DecodeRecovery.Action, Bool) in
            let act = decodeRecovery.step(isSync: isSync, needsFlush: needsFlush, error: errText,
                                          now: ProcessInfo.processInfo.systemUptime)
            return (act, decodeRecovery.waitingForKeyframe)
        }
        if let msg = action.logError { onDecodeError?(msg) }
        if action.flush { renderer.flush() }
        if action.requestKeyframe {
            onDecodeFailure?()
            armStallWatchdog()
        } else if !stillWaiting && !needsFlush {
            disarmStallWatchdog()
        }
        if action.restartSession { onDecodeStall?() }
        guard action.enqueue else { return nil }
        renderer.enqueue(buffer)
        guard let format = CMSampleBufferGetFormatDescription(buffer) else { return nil }
        let dims = CMVideoFormatDescriptionGetDimensions(format)
        let size = CGSize(width: CGFloat(dims.width), height: CGFloat(dims.height))
        let (known, changed, waiting) = sizes.withLock { () -> (CGSize?, Bool, Bool) in
            guard let screen, screen.width > 0 else { return (nil, false, waits) }
            defer { coded = size }
            return (screen, coded != size, false)
        }
        guard var screen = known else { return waiting ? nil : size }
        if changed {
            DispatchQueue.main.async {
                // A capture fallback since then shows whole frames again
                guard self.sizes.withLock({ self.coded == size }) else { return }
                self.display.videoGravity = .resize
                self.display.autoresizingMask = []
                self.needsLayout = true
            }
        }
        if (size.width > size.height) != (screen.width > screen.height) {
            screen = CGSize(width: screen.height, height: screen.width)
        }
        return screen
    }

    /// Wipes the last displayed frame to black and resets decode recovery,
    /// so a disconnected or restarting phone never leaves a stale image on screen
    func clear() {
        decodeLock.withLock { decodeRecovery.reset() }
        disarmStallWatchdog()
        display.sampleBufferRenderer.flush(removingDisplayedImage: true, completionHandler: nil)
    }

    /// Back to whole, unpadded frames filling the view, for the capture path
    /// after the Device Hub stream failed
    func resetForCapture() {
        sizes.withLock {
            screen = nil
            coded = .zero
            waits = false
        }
        display.videoGravity = .resizeAspectFill
        display.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
        needsLayout = true
    }

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
        let size: CGFloat = 18
        let image = NSImage(size: NSSize(width: size, height: size), flipped: false) { rect in
            let circle = NSBezierPath(ovalIn: rect.insetBy(dx: 2, dy: 2))
            NSColor(white: 0.5, alpha: fill).setFill()
            circle.fill()
            NSColor(white: 1, alpha: 0.9).setStroke()
            circle.lineWidth = 1.5
            circle.stroke()
            // A faint dark rim keeps it visible on white
            let rim = NSBezierPath(ovalIn: rect.insetBy(dx: 1, dy: 1))
            NSColor(white: 0, alpha: 0.22).setStroke()
            rim.lineWidth = 1
            rim.stroke()
            return true
        }
        return NSCursor(image: image, hotSpot: NSPoint(x: size / 2, y: size / 2))
    }

    private var commandHeld = false
    /// The round touch pointer over the phone, or macOS's own (`--cursor system`)
    var touchPointer = true

    private var currentCursor: NSCursor {
        if commandPress { return .closedHand }
        if commandHeld { return .openHand }
        guard touchPointer else { return .arrow }
        return pressed ? Self.pressedCursor : Self.touchCursor
    }

    override func resetCursorRects() { addCursorRect(bounds, cursor: currentCursor) }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .activeAlways, .inVisibleRect, .cursorUpdate],
                                       owner: self))
    }

    override func cursorUpdate(with event: NSEvent) {
        guard let window, NSWindow.windowNumber(at: NSEvent.mouseLocation, belowWindowWithWindowNumber: 0)
            == window.windowNumber else { return }
        currentCursor.set()
    }
    override func mouseMoved(with event: NSEvent) { setCommand(event.modifierFlags.contains(.command)) }
    override func flagsChanged(with event: NSEvent) { setCommand(event.modifierFlags.contains(.command)) }

    private func setCommand(_ held: Bool) {
        commandHeld = held
        window?.invalidateCursorRects(for: self)
        // Only where this window is the one under the pointer: the title bar
        // and other windows over it keep their own cursor
        guard let window, bounds.contains(convert(window.mouseLocationOutsideOfEventStream, from: nil)),
              NSWindow.windowNumber(at: NSEvent.mouseLocation, belowWindowWithWindowNumber: 0) == window.windowNumber
        else { return }
        currentCursor.set()
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
        window?.makeFirstResponder(self)
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

    /// Android's Back, as in scrcpy
    override func rightMouseDown(with event: NSEvent) { input?.button("back") }

    override func scrollWheel(with event: NSEvent) {
        guard !event.modifierFlags.contains(.command) else { return }
        // The finger moves with the content, as macOS already turned the deltas
        let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 10
        input?.scroll(at: phonePoint(event), by: CGVector(dx: event.scrollingDeltaX * scale / max(bounds.width, 1),
                                                          dy: event.scrollingDeltaY * scale / max(bounds.height, 1)))
    }

    override func keyDown(with event: NSEvent) {
        let modifiers = event.modifierFlags.intersection([.command, .control, .option, .shift])
        if modifiers == .command, let command = ["v": ClipboardCommand.paste, "c": .copy, "x": .cut][
            event.charactersIgnoringModifiers?.lowercased() ?? ""] {
            input?.clipboard(command)
            return
        }
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
