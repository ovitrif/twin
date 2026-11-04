import AppKit
import Testing
@testable import MirrorUI

private final class Recorder: InputSink {
    var onError: ((String) -> Void)?
    var touches: [String] = []
    var typed: [String] = []
    func touch(_ phase: TouchPhase, at point: CGPoint) { touches.append("\(phase.rawValue) \(point.x) \(point.y)") }
    func scroll(at point: CGPoint, by delta: CGVector) {}
    func type(_ text: String) { typed.append(text) }
    func key(_ usage: Int) { typed.append("key \(usage)") }
    func button(_ name: String) {}
}

/// A 300 × 600 mirror in a window that is never shown
@MainActor private func makeView() -> (MirrorView, Recorder, NSWindow) {
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 600),
                          styleMask: [.borderless], backing: .buffered, defer: true)
    let view = MirrorView(frame: NSRect(x: 0, y: 0, width: 300, height: 600))
    window.contentView = view
    let recorder = Recorder()
    view.input = recorder
    return (view, recorder, window)
}

private func mouse(_ type: NSEvent.EventType, _ x: CGFloat, _ y: CGFloat, in window: NSWindow) -> NSEvent {
    // Window coordinates start at the bottom left
    NSEvent.mouseEvent(with: type, location: NSPoint(x: x, y: y), modifierFlags: [], timestamp: 0,
                       windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
}

private func key(_ chars: String, code: UInt16, flags: NSEvent.ModifierFlags = []) -> NSEvent {
    NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: 0,
                     context: nil, characters: chars, charactersIgnoringModifiers: chars, isARepeat: false,
                     keyCode: code)!
}

@MainActor @Test func aDragReachesThePhoneLiveFromTheTopLeft() {
    let (view, recorder, window) = makeView()
    view.mouseDown(with: mouse(.leftMouseDown, 150, 450, in: window))
    view.mouseDragged(with: mouse(.leftMouseDragged, 150, 300, in: window))
    view.mouseUp(with: mouse(.leftMouseUp, 150, 150, in: window))
    #expect(recorder.touches == ["down 0.5 0.25", "move 0.5 0.5", "up 0.5 0.75"])
}

@MainActor @Test func aPointerOutsideTheScreenStaysOnItsEdge() {
    let (view, recorder, window) = makeView()
    view.mouseDown(with: mouse(.leftMouseDown, 10, 590, in: window))
    view.mouseUp(with: mouse(.leftMouseUp, 400, -20, in: window))
    #expect(recorder.touches.last == "up 1.0 1.0")
}

@MainActor @Test func typingGoesToThePhoneButShortcutsDoNot() {
    let (view, recorder, _) = makeView()
    view.keyDown(with: key("a", code: 0))
    view.keyDown(with: key("\u{7F}", code: 51))
    view.keyDown(with: key("\r", code: 36))
    view.keyDown(with: key("\u{F700}", code: 126))  // up arrow
    view.keyDown(with: key("a", code: 0, flags: .command))
    #expect(recorder.typed == ["a", "\u{8}", "\n", "key 82"])
}

/// dmb-hid stand-in that records the JSON lines HIDInput writes
@MainActor @Test func aNudgeOfTheTrackpadNeverTapsThePhone() throws {
    let script = FileManager.default.temporaryDirectory.appendingPathComponent("dmb-hid-\(UUID()).sh")
    let log = script.appendingPathExtension("log")
    try "#!/bin/sh\necho ready\ncat > '\(log.path)'\n".write(to: script, atomically: true, encoding: .utf8)
    chmod(script.path, 0o755)
    let hid = HIDInput(helper: script, udid: "TEST")
    try hid.start()
    hid.scroll(at: CGPoint(x: 0.5, y: 0.5), by: CGVector(dx: 0, dy: 0.005))
    hid.scroll(at: CGPoint(x: 0.5, y: 0.5), by: CGVector(dx: 0, dy: 0.005))
    RunLoop.main.run(until: Date().addingTimeInterval(0.3))  // the scroll pauses
    hid.scroll(at: CGPoint(x: 0.5, y: 0.5), by: CGVector(dx: 0, dy: 0.02))
    hid.scroll(at: CGPoint(x: 0.5, y: 0.5), by: CGVector(dx: 0, dy: 0.02))
    RunLoop.main.run(until: Date().addingTimeInterval(0.3))
    hid.stop()
    let phases = try String(contentsOf: log, encoding: .utf8).split(separator: "\n")
        .compactMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] }
        .compactMap { $0["touch"] as? String }
    #expect(phases == ["down", "move", "move", "up"])
}
