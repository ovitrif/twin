import AppKit
import IMBCore

let usage = """
imb: mirror a USB-connected iPhone in a QuickTime-style window

  imb status [--json]          list iPhones on USB and whether each is mirrored
  imb mirror [--udid UDID]     open the mirror (the first iPhone without --udid)
             [--wda URL]       send clicks and typing to WebDriverAgent instead
             [--exec]          become the mirror process, for apps that track it (awb)
  imb --version
"""

let stateDir = FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent(".local/state/imb", isDirectory: true)

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("imb: \(message)\n".utf8))
    exit(1)
}

func pidFile(_ udid: String) -> URL { stateDir.appendingPathComponent("mirror-\(udid).pid") }

/// The running mirror for this phone, if any.
func mirrorPID(_ udid: String) -> pid_t? {
    guard let text = try? String(contentsOf: pidFile(udid), encoding: .utf8),
          let pid = pid_t(text.trimmingCharacters(in: .whitespacesAndNewlines)),
          kill(pid, 0) == 0,
          NSRunningApplication(processIdentifier: pid) != nil
    else { return nil }
    return pid
}

/// iOS Mirror's binary: inside the app next to this one, the installed app,
/// or the bare build product in development.
func mirrorBinary() -> String? {
    let exe = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath().deletingLastPathComponent()
    let home = FileManager.default.homeDirectoryForCurrentUser.path
    let candidates: [String?] = [
        ProcessInfo.processInfo.environment["IMB_MIRROR"],
        exe.appendingPathComponent("iOS Mirror.app/Contents/MacOS/iOS Mirror").path,
        exe.appendingPathComponent("../iOS Mirror.app/Contents/MacOS/iOS Mirror").standardized.path,
        "/Applications/iOS Mirror.app/Contents/MacOS/iOS Mirror",
        "\(home)/Applications/iOS Mirror.app/Contents/MacOS/iOS Mirror",
        exe.appendingPathComponent("IOSMirror").path,
    ]
    return candidates.compactMap { $0 }.first { FileManager.default.isExecutableFile(atPath: $0) }
}

func status(json: Bool) {
    let devices: [Device]
    do { devices = try Devices.list().filter(\.wired) } catch { fail("\(error)") }
    if json {
        struct Row: Encodable { var udid, name, model, productType: String; var mirroring: Bool }
        let rows = devices.map { Row(udid: $0.udid, name: $0.name, model: $0.model,
                                     productType: $0.productType, mirroring: mirrorPID($0.udid) != nil) }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        print(String(decoding: try! encoder.encode(["devices": rows]), as: UTF8.self))
        return
    }
    if devices.isEmpty { print("No iPhone on USB. Plug one in and tap Trust on it."); return }
    for d in devices {
        print("\(d.name)  \(d.model)  \(d.udid)\(mirrorPID(d.udid) != nil ? "  mirroring" : "")")
    }
}

func mirror(_ args: [String]) {
    var udid: String?
    var exec = false
    var passthrough: [String] = []
    var it = args.makeIterator()
    while let arg = it.next() {
        switch arg {
        case "--udid": udid = it.next()
        case "--exec": exec = true
        default: passthrough.append(arg)
        }
    }
    let devices: [Device]
    do { devices = try Devices.list().filter(\.wired) } catch { fail("\(error)") }
    guard let device = udid.map({ u in devices.first { $0.udid == u } }) ?? devices.first else {
        fail(udid.map { "no iPhone with UDID \($0) on USB" } ?? "no iPhone on USB; plug one in and tap Trust on it")
    }
    if let pid = mirrorPID(device.udid) {
        NSRunningApplication(processIdentifier: pid)?.activate()
        print("Already mirroring \(device.name)")
        return
    }
    guard let binary = mirrorBinary() else { fail("iOS Mirror.app not found; reinstall imb") }
    let arguments = ["--name", device.name, "--udid", device.udid, "--product", device.productType] + passthrough
    try? FileManager.default.createDirectory(at: stateDir, withIntermediateDirectories: true)
    if exec {
        // The caller's child becomes the mirror, so stopping it closes the mirror
        try? "\(getpid())\n".write(to: pidFile(device.udid), atomically: true, encoding: .utf8)
        let argv = ([binary] + arguments).map { strdup($0) } + [nil]
        execv(binary, argv)
        fail("cannot start iOS Mirror: \(String(cString: strerror(errno)))")
    }
    let process = Process()
    process.executableURL = URL(fileURLWithPath: binary)
    process.arguments = arguments
    process.standardOutput = FileHandle.nullDevice
    let log = stateDir.appendingPathComponent("mirror-\(device.udid).log")
    FileManager.default.createFile(atPath: log.path, contents: nil)
    process.standardError = try? FileHandle(forWritingTo: log)
    do { try process.run() } catch { fail("cannot start iOS Mirror: \(error.localizedDescription)") }
    try? "\(process.processIdentifier)\n".write(to: pidFile(device.udid), atomically: true, encoding: .utf8)
    print("Mirroring \(device.name)")
}

var args = Array(CommandLine.arguments.dropFirst())
switch args.first {
case "status", "list": status(json: args.contains("--json"))
case "mirror": mirror(Array(args.dropFirst()))
case "--version", "-V": print("imb 0.1.0")
case nil, "-h", "--help", "help": print(usage)
default: fail("unknown command \(args[0])\n\n\(usage)")
}
