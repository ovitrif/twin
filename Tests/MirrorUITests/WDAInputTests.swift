import Foundation
import Testing
@testable import IMBCore
@testable import MirrorUI

/// scripts/fake-wda.py on a loopback port, logging each request it gets
private func fakeWDA() throws -> (Process, URL, URL) {
    let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../..").standardized
    let log = FileManager.default.temporaryDirectory.appendingPathComponent("imb-wda-\(UUID()).log")
    let port = Int.random(in: 20000...40000)
    let server = Process()
    server.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
    server.arguments = [root.appendingPathComponent("scripts/fake-wda.py").path, "\(port)", log.path]
    try server.run()
    return (server, URL(string: "http://127.0.0.1:\(port)")!, log)
}

/// The logged requests other than readiness checks, once there are `count`
/// of them or three seconds have passed
private func requests(in log: URL, count: Int) async throws -> [[String: Any]] {
    var found: [[String: Any]] = []
    for _ in 0..<60 {
        let lines = (try? String(contentsOf: log, encoding: .utf8))?.split(separator: "\n") ?? []
        found = lines.compactMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] }
            .filter { $0["path"] as? String != "/status" }
        if found.count >= count { break }
        try await Task.sleep(for: .milliseconds(50))
    }
    return found
}

@MainActor @Test func aTouchReachesWebDriverAgentAsOneGestureInPoints() async throws {
    let (server, url, log) = try fakeWDA()
    defer { server.terminate() }
    // Wait for the server before the first request
    for _ in 0..<60 where (try? Data(contentsOf: url.appendingPathComponent("status"))) == nil {
        try await Task.sleep(for: .milliseconds(50))
    }
    let input = WDAInput(client: WDAClient(base: url))
    var errors: [String] = []
    input.onError = { errors.append($0) }
    _ = try await requests(in: log, count: 2)  // session and screen size
    input.touch(.down, at: CGPoint(x: 0.5, y: 0.25))
    input.touch(.move, at: CGPoint(x: 0.5, y: 0.5))
    input.touch(.up, at: CGPoint(x: 0.5, y: 0.75))
    let all = try await requests(in: log, count: 3)
    #expect(errors.isEmpty)
    #expect(all.map { $0["path"] as? String } == ["/session", "/session/fake/window/size", "/session/fake/actions"])
    let pointer = ((all.last?["body"] as? [String: Any])?["actions"] as? [[String: Any]])?.first
    let moves = (pointer?["actions"] as? [[String: Any]] ?? []).filter { $0["type"] as? String == "pointerMove" }
    // 420 × 912 points, the fake's iPhone Air
    #expect(moves.map { [$0["x"] as? Int, $0["y"] as? Int] } == [[210, 228], [210, 456], [210, 684]])
}
