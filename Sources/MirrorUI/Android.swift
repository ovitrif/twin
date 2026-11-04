import AppKit
import AVFoundation
import DMBCore

/// An Android phone's screen through the scrcpy client.
final class ScrcpySource: FrameSource {
    var onFrame: ((CMSampleBuffer) -> Void)?
    var onStatus: ((String) -> Void)?
    let session: ScrcpySession

    init(session: ScrcpySession) { self.session = session }

    func start() {
        onStatus?("Connecting to \(session.serial)…")
        session.onFrame = { [weak self] in self?.onFrame?($0) }
        session.onStatus = { [weak self] text in DispatchQueue.main.async { self?.onStatus?(text) } }
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            do { try self.session.start() } catch {
                DispatchQueue.main.async { self.onStatus?("\(error)") }
            }
        }
    }

    func stop() { session.stop() }
}

/// Touch, keys and buttons for an Android phone, in scrcpy's control format.
final class ScrcpyInput: InputSink {
    var onError: ((String) -> Void)?
    private let session: ScrcpySession
    private var scrollPending = CGVector.zero

    init(session: ScrcpySession) { self.session = session }

    private func position(_ p: CGPoint) -> (Int32, Int32, UInt16, UInt16)? {
        let size = session.videoSize
        guard size.width > 0 else { return nil }
        return (Int32((p.x * size.width).rounded()), Int32((p.y * size.height).rounded()),
                UInt16(size.width), UInt16(size.height))
    }

    func touch(_ phase: TouchPhase, at point: CGPoint) {
        guard let (x, y, w, h) = position(point) else { return }
        let action: ScrcpyProtocol.TouchAction = phase == .down ? .down : (phase == .up ? .up : .move)
        session.send(ScrcpyProtocol.touch(action, x: x, y: y, width: w, height: h))
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
        session.send(ScrcpyProtocol.scroll(x: x, y: y, width: w, height: h, h: Float(-hs), v: Float(vs)))
    }

    func type(_ text: String) {
        switch text {
        case "\u{8}": tap(67)       // KEYCODE_DEL
        case "\u{7F}": tap(112)     // KEYCODE_FORWARD_DEL
        case "\n": tap(66)          // KEYCODE_ENTER
        case "\t": tap(61)          // KEYCODE_TAB
        default: session.send(ScrcpyProtocol.text(text))
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
            session.send(ScrcpyProtocol.back(down: true))
            session.send(ScrcpyProtocol.back(down: false))
        default: break
        }
    }

    private func tap(_ keycode: UInt32) {
        session.send(ScrcpyProtocol.key(keycode, down: true))
        session.send(ScrcpyProtocol.key(keycode, down: false))
    }
}
