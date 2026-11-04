import CoreGraphics
import Foundation

/// One touch from press to release, in phone points, with the time of each
/// point in seconds since the press.
public struct Gesture: Equatable {
    public var points: [CGPoint]
    public var times: [TimeInterval]

    public init(points: [CGPoint], times: [TimeInterval]) {
        self.points = points
        self.times = times
    }

    /// W3C pointer actions that replay the touch with its own timing.
    public func actions() -> [String: Any] {
        guard let start = points.first else { return ["actions": []] }
        var steps: [[String: Any]] = [
            ["type": "pointerMove", "duration": 0, "x": Int(start.x), "y": Int(start.y)],
            ["type": "pointerDown", "button": 0],
        ]
        var last: TimeInterval = 0
        for (point, time) in zip(points.dropFirst(), times.dropFirst()) {
            steps.append(["type": "pointerMove", "duration": max(0, Int(((time - last) * 1000).rounded())),
                          "x": Int(point.x), "y": Int(point.y)])
            last = time
        }
        // A press held still ends with a pause as long as the hold
        if let end = times.last, end - last > 0.01 {
            steps.append(["type": "pause", "duration": Int(((end - last) * 1000).rounded())])
        }
        steps.append(["type": "pointerUp", "button": 0])
        return ["actions": [["type": "pointer", "id": "finger1",
                             "parameters": ["pointerType": "touch"], "actions": steps]]]
    }
}

/// A WebDriverAgent server reached over HTTP (`iproxy 8100` for a phone).
public final class WDAClient {
    public let base: URL
    private var sessionID: String?
    public private(set) var screenSize: CGSize?

    public init(base: URL) { self.base = base }

    public func connect() async throws {
        let created = try await call("POST", "session", ["capabilities": ["alwaysMatch": [:]]])
        guard let id = created["sessionId"] as? String
            ?? (created["value"] as? [String: Any])?["sessionId"] as? String
        else { throw DMBError("WebDriverAgent returned no session") }
        sessionID = id
        let size = try await call("GET", "session/\(id)/window/size", nil)
        if let value = size["value"] as? [String: Any],
           let w = value["width"] as? Double, let h = value["height"] as? Double {
            screenSize = CGSize(width: w, height: h)
        }
    }

    public func perform(_ gesture: Gesture) async throws {
        _ = try await call("POST", "session/\(try session())/actions", gesture.actions())
    }

    public func type(_ text: String) async throws {
        _ = try await call("POST", "session/\(try session())/wda/keys", ["value": text.map(String.init)])
    }

    public func home() async throws {
        _ = try await call("POST", "wda/homescreen", [:])
    }

    private func session() throws -> String {
        guard let sessionID else { throw DMBError("not connected to WebDriverAgent") }
        return sessionID
    }

    @discardableResult
    private func call(_ method: String, _ path: String, _ body: [String: Any]?) async throws -> [String: Any] {
        var request = URLRequest(url: base.appendingPathComponent(path))
        request.httpMethod = method
        request.timeoutInterval = 30
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        if let status = (response as? HTTPURLResponse)?.statusCode, status >= 400 {
            let message = ((json["value"] as? [String: Any])?["message"] as? String) ?? "HTTP \(status)"
            throw DMBError("WebDriverAgent \(path): \(message)")
        }
        return json
    }
}
