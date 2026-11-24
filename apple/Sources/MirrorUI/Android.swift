import AppKit
import AVFoundation
import DMBCore

/// The surface `ScrcpySource` and `ScrcpyInput` need from a scrcpy session,
/// so reconnect and input handoff can be tested without adb or sockets.
protocol ScrcpyClient: AnyObject {
    var videoSize: CGSize { get }
    var onFrame: ((CMSampleBuffer) -> Void)? { get set }
    var onStatus: ((String) -> Void)? { get set }
    var onClipboard: ((String) -> Void)? { get set }
    var onEnded: ((Error?) -> Void)? { get set }
    func start() throws
    func stop()
    func send(_ message: Data)
}

extension ScrcpySession: ScrcpyClient {}

/// An Android phone's screen through the scrcpy client; when the phone drops,
/// it notifies `onDisconnected` and watches adb to reconnect automatically.
final class ScrcpySource: FrameSource {
    var onFrame: ((CMSampleBuffer) -> Void)?
    var onStatus: ((String) -> Void)?
    var onDisconnected: (() -> Void)?
    var onClipboard: ((String) -> Void)?
    var onError: ((String) -> Void)?
    let serial: String

    private let makeSession: () -> ScrcpyClient
    private let isReady: () -> Bool
    private let retryDelay: TimeInterval
    private let queue = DispatchQueue(label: "dmb.scrcpy.source", qos: .userInitiated)
    private let lock = NSLock()
    private var current: ScrcpyClient?
    private var stopped = false
    private var retryWork: DispatchWorkItem?

    /// The live scrcpy session, or nil while disconnected/reconnecting
    var activeSession: ScrcpyClient? { lock.withLock { current } }

    convenience init(serial: String, adb: String, server: String, maxSize: Int = 0, retryDelay: TimeInterval = 1.5) {
        self.init(
            serial: serial,
            retryDelay: retryDelay,
            isReady: { ScrcpySession.isDeviceReady(adb: adb, serial: serial) },
            makeSession: { ScrcpySession(serial: serial, adb: adb, server: server, maxSize: maxSize) }
        )
    }

    init(serial: String, retryDelay: TimeInterval = 1.5,
         isReady: @escaping () -> Bool, makeSession: @escaping () -> ScrcpyClient) {
        self.serial = serial
        self.retryDelay = retryDelay
        self.isReady = isReady
        self.makeSession = makeSession
    }

    func start() {
        lock.withLock { stopped = false }
        onStatus?("Connecting…")
        attempt(initial: true)
    }

    func stop() {
        let session = lock.withLock { () -> ScrcpyClient? in
            stopped = true
            retryWork?.cancel()
            retryWork = nil
            defer { current = nil }
            return current
        }
        session?.stop()
    }

    private func attempt(initial: Bool) {
        queue.async { [weak self] in
            guard let self else { return }
            guard self.lock.withLock({ !self.stopped }) else { return }
            if !initial && !self.isReady() {
                self.scheduleRetry()
                return
            }
            let session = self.makeSession()
            session.onFrame = { [weak self] in self?.onFrame?($0) }
            session.onClipboard = { [weak self] in self?.onClipboard?($0) }
            session.onEnded = { [weak self, weak session] error in
                guard let self, let session else { return }
                let dropped = self.lock.withLock { () -> Bool in
                    guard !self.stopped, self.current === session else { return false }
                    self.current = nil
                    return true
                }
                guard dropped else { return }
                if let error { self.onError?("Android stream ended: \(error)") }
                DispatchQueue.main.async { self.onDisconnected?() }
                self.scheduleRetry()
            }
            let installed = self.lock.withLock { () -> Bool in
                guard !self.stopped else { return false }
                self.current = session
                return true
            }
            guard installed else { session.stop(); return }
            do {
                try session.start()
            } catch {
                let active = self.lock.withLock { () -> Bool in
                    if self.current === session { self.current = nil }
                    return !self.stopped
                }
                guard active else { return }
                self.onError?("Android connect failed: \(error)")
                if initial {
                    DispatchQueue.main.async { self.onDisconnected?() }
                }
                self.scheduleRetry()
            }
        }
    }

    private func scheduleRetry() {
        let work = DispatchWorkItem { [weak self] in self?.attempt(initial: false) }
        let scheduled = lock.withLock { () -> Bool in
            guard !stopped else { return false }
            retryWork?.cancel()
            retryWork = work
            return true
        }
        guard scheduled else { return }
        queue.asyncAfter(deadline: .now() + retryDelay, execute: work)
    }
}

/// Touch, keys and buttons for an Android phone, in scrcpy's control format.
/// Reads the active session from `ScrcpySource` so input and clipboard resume
/// seamlessly after a reconnect.
final class ScrcpyInput: InputSink {
    var onError: ((String) -> Void)?
    private weak var source: ScrcpySource?
    private var scrollPending = CGVector.zero

    init(source: ScrcpySource) {
        self.source = source
        // What the phone copies lands on the Mac's clipboard, as in scrcpy
        source.onClipboard = { [weak self] text in
            DispatchQueue.main.async {
                // The phone echoes a paste back; text the Mac already holds
                // stays as it is, anything else lands
                guard self != nil, NSPasteboard.general.string(forType: .string) != text else { return }
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(text, forType: .string)
            }
        }
    }

    private var session: ScrcpyClient? { source?.activeSession }

    /// ⌘V hands the Mac's clipboard to the phone and pastes it; ⌘C and ⌘X
    /// copy or cut on the phone, whose clipboard then comes back to the Mac
    func clipboard(_ command: ClipboardCommand) {
        switch command {
        case .paste:
            guard let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else { return }
            session?.send(ScrcpyProtocol.setClipboard(text, paste: true))
        case .copy: tap(278)  // KEYCODE_COPY
        case .cut: tap(277)   // KEYCODE_CUT
        }
    }

    private func position(_ p: CGPoint) -> (Int32, Int32, UInt16, UInt16)? {
        guard let session else { return nil }
        return Self.position(p, in: session.videoSize)
    }

    /// A point from 0 to 1 in pixels of a video this size, with the size as
    /// scrcpy takes it; nil before the first frame or for a size it cannot carry
    static func position(_ p: CGPoint, in size: CGSize) -> (Int32, Int32, UInt16, UInt16)? {
        let limit = CGFloat(UInt16.max)
        guard size.width > 0, size.height > 0, size.width <= limit, size.height <= limit else { return nil }
        return (Int32((p.x * size.width).rounded()), Int32((p.y * size.height).rounded()),
                UInt16(size.width), UInt16(size.height))
    }

    func touch(_ phase: TouchPhase, at point: CGPoint) {
        guard let (x, y, w, h) = position(point) else { return }
        let action: ScrcpyProtocol.TouchAction = phase == .down ? .down : (phase == .up ? .up : .move)
        session?.send(ScrcpyProtocol.touch(action, x: x, y: y, width: w, height: h))
    }

    /// A wheel or trackpad scroll as Android mouse wheel steps, about one per
    /// 2.5% of the screen
    func scroll(at point: CGPoint, by delta: CGVector) {
        guard let (x, y, w, h) = position(point) else { return }
        scrollPending.dx += delta.dx * 40
        scrollPending.dy += delta.dy * 40
        let hs = scrollPending.dx.rounded(.towardZero), vs = scrollPending.dy.rounded(.towardZero)
        guard hs != 0 || vs != 0 else { return }
        scrollPending.dx -= hs
        scrollPending.dy -= vs
        session?.send(ScrcpyProtocol.scroll(x: x, y: y, width: w, height: h, h: Float(-hs), v: Float(vs)))
    }

    func type(_ text: String) {
        switch text {
        case "\u{8}": tap(67)       // KEYCODE_DEL
        case "\u{7F}": tap(112)     // KEYCODE_FORWARD_DEL
        case "\n": tap(66)          // KEYCODE_ENTER
        case "\t": tap(61)          // KEYCODE_TAB
        default: session?.send(ScrcpyProtocol.text(text))
        }
    }

    func key(_ usage: Int) {
        // HID usages from the view: escape, then arrows right, left, down, up
        let keycodes = [0x29: 111, 0x4F: 22, 0x50: 21, 0x51: 20, 0x52: 19]
        if let code = keycodes[usage] { tap(UInt32(code)) }
    }

    func button(_ name: String) {
        switch name {
        case "home": tap(3)    // KEYCODE_HOME
        case "lock": tap(26)   // KEYCODE_POWER
        case "back":
            session?.send(ScrcpyProtocol.back(down: true))
            session?.send(ScrcpyProtocol.back(down: false))
        default: break
        }
    }

    private func tap(_ keycode: UInt32) {
        session?.send(ScrcpyProtocol.key(keycode, down: true))
        session?.send(ScrcpyProtocol.key(keycode, down: false))
    }
}
