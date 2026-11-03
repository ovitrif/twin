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
    #expect(recorder.typed == ["a", "\u{8}", "\n"])
}
