import AppKit
import DMBCore

// What the one-phone window (App.swift) and the pair window (Pair.swift)
// share, so the two never drift apart.

/// Quits the app when `parent` ends (dmb's --wait); keep the source alive
func watchParent(_ parent: pid_t?) -> DispatchSourceProcess? {
    guard let parent else { return nil }
    let watch = DispatchSource.makeProcessSource(identifier: parent, eventMask: .exit, queue: .main)
    watch.setEventHandler { NSApp.terminate(nil) }
    watch.resume()
    if kill(parent, 0) != 0 { NSApp.terminate(nil) }
    return watch
}

/// dmb-hid sits next to this binary in the app bundle; DMB_HID overrides it
var hidHelper: URL? {
    let candidates = [ProcessInfo.processInfo.environment["DMB_HID"],
                      Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("dmb-hid").path]
    return candidates.compactMap { $0 }.first { FileManager.default.isExecutableFile(atPath: $0) }
        .map { URL(fileURLWithPath: $0) }
}

/// The app menu with Quit, and the Device menu: Home, Lock and, when given,
/// Back go to `target`
func buildMenu(target: AnyObject, home: Selector, lock: Selector, back: Selector? = nil) {
    let main = NSMenu()
    let appItem = NSMenuItem()
    let app = NSMenu()
    app.addItem(withTitle: "Quit Twin", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
    appItem.submenu = app
    main.addItem(appItem)
    let deviceItem = NSMenuItem()
    let device = NSMenu(title: "Device")
    device.addItem(withTitle: "Home", action: home, keyEquivalent: "h").target = target
    device.addItem(withTitle: "Lock", action: lock, keyEquivalent: "l").target = target
    if let back { device.addItem(withTitle: "Back", action: back, keyEquivalent: "b").target = target }
    device.addItem(withTitle: "Close", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "w")
    device.addItem(withTitle: "Minimize", action: #selector(NSWindow.miniaturize(_:)), keyEquivalent: "m")
    deviceItem.submenu = device
    main.addItem(deviceItem)
    NSApp.mainMenu = main
}

/// The iPhone's screen and touch over one Device Hub session, shown in
/// `view`; `onFailed` runs on a phone with no Device Hub (before iOS 27),
/// for the caller to capture the screen instead
func deviceHub(helper: URL, udid: String, view: MirrorView,
               onFailed: @escaping () -> Void) -> (HIDInput, DeviceHubSource) {
    let hid = HIDInput(helper: helper, udid: udid, video: true)
    let hub = DeviceHubSource(hid: hid)
    hub.onScreenSize = { [weak view] in view?.screenSize = $0 }
    hid.onFailed = { [weak view] in
        view?.screenSize = nil
        onFailed()
    }
    return (hid, hub)
}
