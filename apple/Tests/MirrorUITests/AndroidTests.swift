import CoreMedia
import Foundation
import Testing
@testable import DMBCore
@testable import MirrorUI

private final class FakeScrcpyClient: ScrcpyClient {
    var videoSize = CGSize(width: 1080, height: 2400)
    var onFrame: ((CMSampleBuffer) -> Void)?
    var onStatus: ((String) -> Void)?
    var onClipboard: ((String) -> Void)?
    var onEnded: ((Error?) -> Void)?
    var started = false
    var stopped = false
    var sent: [Data] = []

    func start() throws { started = true }
    func stop() { stopped = true }
    func send(_ message: Data) { sent.append(message) }
}

@Test func touchesWaitForAVideoSizeScrcpyCanCarry() {
    #expect(ScrcpyInput.position(CGPoint(x: 0.5, y: 0.25), in: CGSize(width: 1080, height: 2400))! == (540, 600, 1080, 2400))
    #expect(ScrcpyInput.position(.zero, in: .zero) == nil)
    #expect(ScrcpyInput.position(.zero, in: CGSize(width: 1080, height: 0)) == nil)
    #expect(ScrcpyInput.position(.zero, in: CGSize(width: 70_000, height: 2400)) == nil)
    #expect(ScrcpyInput.position(.zero, in: CGSize(width: 1080, height: 70_000)) == nil)
}

@MainActor @Test func scrcpySourceReconnectsAndHandsInputToNewSession() async throws {
    var ready = false
    var created: [FakeScrcpyClient] = []
    let source = ScrcpySource(serial: "TEST", retryDelay: 0.05, isReady: { ready }, makeSession: {
        let client = FakeScrcpyClient()
        created.append(client)
        return client
    })
    let input = ScrcpyInput(source: source)
    var disconnects = 0
    source.onDisconnected = { disconnects += 1 }
    source.start()

    let waitUntil: (() -> Bool) async throws -> Void = { condition in
        let deadline = Date().addingTimeInterval(2)
        while !condition() && Date() < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
    }

    try await waitUntil { created.count == 1 && created[0].started }
    let first = created[0]
    input.button("home")
    #expect(first.sent.count == 2)

    // Phone disconnects: notifies once and clears active session so input no-ops
    first.onEnded?(DMBError("scrcpy stream ended"))
    try await waitUntil { disconnects == 1 }
    #expect(source.activeSession == nil)
    input.button("home")
    #expect(first.sent.count == 2)

    // Phone returns on adb: source creates a new session and input routes to it
    ready = true
    try await waitUntil { created.count == 2 && created[1].started }
    let second = created[1]
    input.button("home")
    #expect(second.sent.count == 2)
    source.stop()
    #expect(second.stopped)
}
