import Foundation

/// An iPhone on this Mac's USB, as `devicectl` reports it.
public struct Device: Codable, Equatable {
    public var udid: String
    public var name: String
    public var model: String
    public var productType: String
    public var wired: Bool
}

public enum Devices {
    /// The physical iOS devices CoreDevice knows, wired or not.
    public static func list() throws -> [Device] {
        let out = FileManager.default.temporaryDirectory
            .appendingPathComponent("dmb-devicectl-\(getpid()).json")
        defer { try? FileManager.default.removeItem(at: out) }
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
        task.arguments = ["devicectl", "list", "devices", "--quiet", "--json-output", out.path]
        task.standardOutput = FileHandle.nullDevice
        task.standardError = FileHandle.nullDevice
        try task.run()
        task.waitUntilExit()
        guard task.terminationStatus == 0 else {
            throw DMBError("devicectl failed (exit \(task.terminationStatus)); is Xcode installed?")
        }
        return try parse(Data(contentsOf: out))
    }

    /// Reads `devicectl list devices --json-output`, preferring its `properties`
    /// dictionary and falling back to the deprecated top-level fields.
    public static func parse(_ data: Data) throws -> [Device] {
        guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let result = root["result"] as? [String: Any],
              let devices = result["devices"] as? [[String: Any]]
        else { throw DMBError("unexpected devicectl output") }
        return devices.compactMap { entry in
            let props = entry["properties"] as? [String: Any] ?? [:]
            let hardware = props["hardware"] as? [String: Any]
                ?? entry["hardwareProperties"] as? [String: Any] ?? [:]
            let connection = props["connection"] as? [String: Any]
                ?? entry["connectionProperties"] as? [String: Any] ?? [:]
            let state = props["state"] as? [String: Any]
                ?? entry["deviceProperties"] as? [String: Any] ?? [:]
            guard hardware["reality"] as? String == "physical",
                  hardware["platform"] as? String == "iOS",
                  let udid = hardware["udid"] as? String
            else { return nil }
            return Device(
                udid: udid,
                name: state["name"] as? String ?? udid,
                model: hardware["marketingName"] as? String ?? "iPhone",
                productType: hardware["productType"] as? String ?? "",
                wired: connection["transportType"] as? String == "wired"
            )
        }
    }
}

public struct DMBError: Error, CustomStringConvertible {
    public var description: String
    public init(_ description: String) { self.description = description }
}
