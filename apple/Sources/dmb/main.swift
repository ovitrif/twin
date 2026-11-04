import AppKit
import DMBCore

let usage = """
dmb: mirror a USB-connected iPhone in a QuickTime-style window

  dmb status [--json]          list iPhones on USB and whether each is mirrored
  dmb mirror [--udid UDID]     open the mirror (the first iPhone without --udid)
             [--wda URL]       send clicks and typing to WebDriverAgent instead
             [--wait]          stay running while the mirror is open; ending dmb
                               closes it, for apps that track it (awb)
  dmb devices --serial SERIAL  show this Android phone beside the iPhone in one
              [--udid UDID]    Twin window; --wait as for mirror
  dmb pane android|iphone show|hide --udid UDID
                               show or hide one phone of the Twin pair showing
                               that iPhone; hiding both closes the window
  dmb --version
"""

let stateDir = FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent(".local/state/dmb", isDirectory: true)

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("dmb: \(message)\n".utf8))
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

/// Twin's binary: inside the app next to this one, the installed app, or the
/// bare build product in development.
func twinBinary() -> String? {
    let exe = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath().deletingLastPathComponent()
    let home = FileManager.default.homeDirectoryForCurrentUser.path
    let candidates: [String?] = [
        ProcessInfo.processInfo.environment["DMB_TWIN"],
        exe.appendingPathComponent("Twin.app/Contents/MacOS/Twin").path,
        exe.appendingPathComponent("../Twin.app/Contents/MacOS/Twin").standardized.path,
        "/Applications/Twin.app/Contents/MacOS/Twin",
        "\(home)/Applications/Twin.app/Contents/MacOS/Twin",
        exe.appendingPathComponent("Twin").path,
    ]
    return candidates.compactMap { $0 }.first { FileManager.default.isExecutableFile(atPath: $0) }
}

/// Starts Twin through LaunchServices when it is an app bundle, so macOS asks
/// camera access for Twin itself rather than for whoever ran dmb.
func launch(_ binary: String, _ arguments: [String]) throws -> pid_t {
    let app = URL(fileURLWithPath: binary).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    guard app.pathExtension == "app" else {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary)
        process.arguments = arguments
        try process.run()
        return process.processIdentifier
    }
    let config = NSWorkspace.OpenConfiguration()
    config.arguments = arguments
    config.createsNewApplicationInstance = true
    config.activates = true
    let done = DispatchSemaphore(value: 0)
    var result: Result<pid_t, Error> = .failure(DMBError("LaunchServices did not answer"))
    NSWorkspace.shared.openApplication(at: app, configuration: config) { running, error in
        if let running { result = .success(running.processIdentifier) } else if let error { result = .failure(error) }
        done.signal()
    }
    done.wait()
    return try result.get()
}

func devices(_ args: [String]) {
    var udid: String?, serial: String?
    var wait = false
    var passthrough: [String] = []
    var it = args.makeIterator()
    while let arg = it.next() {
        switch arg {
        case "--udid": udid = it.next()
        case "--serial": serial = it.next()
        case "--wait": wait = true
        default: passthrough.append(arg)
        }
    }
    guard let serial else { fail("devices needs --serial, the Android phone's adb serial") }
    let phones: [Device]
    do { phones = try Devices.list().filter(\.wired) } catch { fail("\(error)") }
    guard let iphone = udid.map({ u in phones.first { $0.udid == u } }) ?? phones.first else {
        fail("no iPhone on USB; plug one in and tap Trust on it")
    }
    guard let binary = twinBinary() else { fail("Twin.app not found; reinstall dmb") }
    try? FileManager.default.createDirectory(at: stateDir, withIntermediateDirectories: true)
    var arguments = ["--serial", serial, "--udid", iphone.udid, "--name", iphone.name, "--product", iphone.productType,
                     "--log", stateDir.appendingPathComponent("devices.log").path] + passthrough
    if wait { arguments += ["--parent", "\(getpid())"] }
    let pid: pid_t
    do { pid = try launch(binary, arguments) } catch { fail("cannot start Twin: \(error.localizedDescription)") }
    print("Showing \(serial) and \(iphone.name)")
    if wait { while kill(pid, 0) == 0 { usleep(250_000) } }
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
    var wait = false
    var passthrough: [String] = []
    var it = args.makeIterator()
    while let arg = it.next() {
        switch arg {
        case "--udid": udid = it.next()
        case "--wait": wait = true
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
        if wait { while kill(pid, 0) == 0 { usleep(250_000) } }
        return
    }
    guard let binary = twinBinary() else { fail("Twin.app not found; reinstall dmb") }
    try? FileManager.default.createDirectory(at: stateDir, withIntermediateDirectories: true)
    let log = stateDir.appendingPathComponent("mirror-\(device.udid).log")
    var arguments = ["--name", device.name, "--udid", device.udid, "--product", device.productType,
                     "--log", log.path] + passthrough
    if wait { arguments += ["--parent", "\(getpid())"] }
    let pid: pid_t
    do { pid = try launch(binary, arguments) } catch { fail("cannot start Twin: \(error.localizedDescription)") }
    try? "\(pid)\n".write(to: pidFile(device.udid), atomically: true, encoding: .utf8)
    if wait {
        print("Mirroring \(device.name)")
        while kill(pid, 0) == 0 { usleep(250_000) }
        return
    }
    print("Mirroring \(device.name)")
}

func pane(_ args: [String]) {
    guard args.count >= 2, let phone = PaneRequest.Phone(rawValue: args[0]), ["show", "hide"].contains(args[1]),
          let index = args.firstIndex(of: "--udid"), index + 1 < args.count else {
        fail("usage: dmb pane android|iphone show|hide --udid UDID")
    }
    PaneRequest(pane: phone, visible: args[1] == "show").post(udid: args[index + 1])
}

var args = Array(CommandLine.arguments.dropFirst())
switch args.first {
case "status", "list": status(json: args.contains("--json"))
case "mirror": mirror(Array(args.dropFirst()))
case "android-probe": androidProbe(Array(args.dropFirst()))
case "devices": devices(Array(args.dropFirst()))
case "pane": pane(Array(args.dropFirst()))
case "iphone-probe": iphoneProbe(Array(args.dropFirst()))
case "--version", "-V": print("dmb 0.1.0")
case nil, "-h", "--help", "help": print(usage)
default: fail("unknown command \(args[0])\n\n\(usage)")
}
