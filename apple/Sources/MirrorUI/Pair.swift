import AppKit
import AVFoundation
import DMBCore

// Twin's two-phone window: an Android phone and an iPhone side by
// side, a gap apart and the same height. One window is one app to macOS, so
// window managers (Raycast's Center), Mission Control and Cmd-Tab treat the
// pair as one, and it moves as one.
//
//   --serial <serial>        the Android phone (adb)
//   --udid <udid>            the iPhone
//   --name <phone name>      the iPhone's capture device name (default: the first iPhone)
//   --product <type>         the iPhone's productType, for its corner radius
//   --height <points>        initial height (default 80% of the screen)
//   --gap <points>           space between the phones (default 12)
//   --android-title <text>   default Android
//   --iphone-title <text>    default iPhone
//   --cursor touch|system    the round touch pointer (default) or macOS's own
//   --server <file>          scrcpy-server 5.0.1 (default: the one in this app)
//   --parent <pid>           quit when this process ends (dmb devices --wait)
//   --log <file>             append diagnostics here instead of stderr
//   --android-replay <file>  play a video in the Android pane instead of a phone, for testing
//   --iphone-replay <file>   the same for the iPhone pane

public struct PairOptions {
    var serial: String?
    var udid: String?
    var name: String?
    var product = ""
    var height: CGFloat?
    var gap: CGFloat = 12
    var androidTitle = "Android"
    var iphoneTitle = "iPhone"
    var cursor = "touch"
    var server: String?
    var parent: pid_t?
    var log: String?
    var androidReplay: String?
    var iphoneReplay: String?

    public init(_ args: [String]) {
        var it = args.makeIterator()
        while let arg = it.next() {
            switch arg {
            case "--serial": serial = it.next()
            case "--udid": udid = it.next()
            case "--name": name = it.next()
            case "--product": product = it.next() ?? ""
            case "--height": height = it.next().flatMap { Double($0) }.map { CGFloat($0) }
            case "--gap": gap = it.next().flatMap { Double($0) }.map { CGFloat($0) } ?? gap
            case "--android-title": androidTitle = it.next() ?? androidTitle
            case "--iphone-title": iphoneTitle = it.next() ?? iphoneTitle
            case "--cursor": cursor = it.next() ?? cursor
            case "--server": server = it.next()
            case "--parent": parent = it.next().flatMap { pid_t($0) }
            case "--log": log = it.next()
            case "--android-replay": androidReplay = it.next()
            case "--iphone-replay": iphoneReplay = it.next()
            default: break
            }
        }
    }

    public var isEmpty: Bool { (serial == nil && androidReplay == nil) || (udid == nil && iphoneReplay == nil) }
}

/// Lays out the two phones: the same height, a gap apart, centered.
private final class PairView: NSView {
    let android = MirrorView(frame: .zero)
    let iphone = MirrorView(frame: .zero)
    var androidAspect: CGFloat = 1080.0 / 2400
    var iphoneAspect: CGFloat = 1260.0 / 2736
    var gap: CGFloat = 12
    var cornerRatio: CGFloat = 0.14
    /// A phone hidden from the pair (`dmb pane`) leaves the window to the other
    var showsAndroid = true
    var showsIPhone = true

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        addSubview(android)
        addSubview(iphone)
    }

    required init?(coder: NSCoder) { nil }

    /// The window width for this height
    func width(forHeight height: CGFloat) -> CGFloat {
        (height * aspects + gaps).rounded()
    }

    private var aspects: CGFloat {
        (showsAndroid ? androidAspect : 0) + (showsIPhone ? iphoneAspect : 0)
    }

    private var gaps: CGFloat { showsAndroid && showsIPhone ? gap : 0 }

    override func layout() {
        super.layout()
        // Full screen leaves room around the pair; fit and center it
        var height = bounds.height
        if width(forHeight: height) > bounds.width {
            height = (bounds.width - gaps) / aspects
        }
        let a = showsAndroid ? (height * androidAspect).rounded() : 0
        let i = showsIPhone ? (height * iphoneAspect).rounded() : 0
        var x = ((bounds.width - (a + gaps + i)) / 2).rounded()
        let y = ((bounds.height - height) / 2).rounded()
        android.isHidden = !showsAndroid
        iphone.isHidden = !showsIPhone
        android.frame = NSRect(x: x, y: y, width: a, height: height)
        x += a + gaps
        iphone.frame = NSRect(x: x, y: y, width: i, height: height)
        let fullScreen = window?.styleMask.contains(.fullScreen) == true
        for pane in [android, iphone] {
            pane.cornerRadius = fullScreen ? 0 : (min(pane.frame.width, pane.frame.height) * cornerRatio).rounded()
        }
    }
}

public final class PairDelegate: NSObject, NSApplicationDelegate {
    private let options: PairOptions
    private var window: MirrorWindow!
    private var pair: PairView!
    private var styler: WindowStyler!
    private var androidSource: FrameSource?
    private var iphoneSource: FrameSource?
    private var hid: HIDInput?
    private var androidInput: ScrcpyInput?
    private var parentWatch: DispatchSourceProcess?
    private var androidSize = CGSize.zero
    private var iphoneSize = CGSize.zero

    public init(options: PairOptions) { self.options = options }

    private func log(_ text: String) { FileHandle.standardError.write(Data("dmb: \(text)\n".utf8)) }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        if let log = options.log { freopen(log, "a", stderr) }
        if let parent = options.parent {
            let watch = DispatchSource.makeProcessSource(identifier: parent, eventMask: .exit, queue: .main)
            watch.setEventHandler { NSApp.terminate(nil) }
            watch.resume()
            parentWatch = watch
            if kill(parent, 0) != 0 { NSApp.terminate(nil) }
        }
        buildMenu()

        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let height = options.height ?? (screen.height * 0.8).rounded()
        pair = PairView(frame: .zero)
        pair.gap = options.gap
        pair.cornerRatio = Screen.cornerRatio(productType: options.product)
        let width = pair.width(forHeight: height)
        window = MirrorWindow(contentRect: NSRect(x: screen.midX - width / 2, y: screen.midY - height / 2,
                                                  width: width, height: height),
                              styleMask: [.borderless, .miniaturizable], backing: .buffered, defer: false)
        window.collectionBehavior = [.fullScreenPrimary]
        window.isReleasedWhenClosed = false
        window.acceptsMouseMovedEvents = true
        window.title = "\(options.androidTitle) and \(options.iphoneTitle)"
        pair.frame = NSRect(x: 0, y: 0, width: width, height: height)
        pair.autoresizingMask = [.width, .height]
        window.contentView = pair
        styler = WindowStyler(window: window, title: window.title, cornerRatio: pair.cornerRatio)
        styler.panes = { [weak self] in
            guard let self else { return [] }
            var panes: [Pane] = []
            if self.pair.showsAndroid {
                panes.append(Pane(frame: self.pair.android.frame, title: self.options.androidTitle))
            }
            if self.pair.showsIPhone {
                panes.append(Pane(frame: self.pair.iphone.frame, title: self.options.iphoneTitle))
            }
            return panes
        }
        for view in [pair.android, pair.iphone] { view.touchPointer = options.cursor != "system" }
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(pair.android)
        styler.applyStyle()
        styler.introduceBar()
        NSApp.activate(ignoringOtherApps: true)

        startAndroid()
        startIPhone()
        listenForPanes()
    }

    /// `dmb pane` hides or shows one phone of this pair, so awb can stop or
    /// start one side without closing the other
    private func listenForPanes() {
        DistributedNotificationCenter.default().addObserver(
            forName: PaneRequest.name, object: options.udid ?? options.iphoneReplay, queue: .main
        ) { [weak self] note in
            guard let self, let request = PaneRequest(note.userInfo) else { return }
            switch request.pane {
            case .android: self.pair.showsAndroid = request.visible
            case .iphone: self.pair.showsIPhone = request.visible
            }
            if !self.pair.showsAndroid && !self.pair.showsIPhone {
                NSApp.terminate(nil)
                return
            }
            self.fitWindow()
        }
    }

    private var serverPath: String? {
        options.server ?? Bundle.main.path(forResource: "scrcpy-server", ofType: nil)
    }

    private func startAndroid() {
        if let replay = options.androidReplay {
            let source = ReplaySource(urls: [URL(fileURLWithPath: replay)])
            source.onFrame = { [weak self] buffer in
                guard let self, let size = self.pair.android.enqueue(buffer) else { return }
                DispatchQueue.main.async { if size != self.androidSize { self.resized(android: size) } }
            }
            androidSource = source
            source.start()
            return
        }
        guard let serial = options.serial, let adb = ScrcpySession.findADB(), let server = serverPath else {
            pair.android.setStatus("adb or scrcpy-server not found")
            return
        }
        let session = ScrcpySession(serial: serial, adb: adb, server: server)
        let source = ScrcpySource(session: session)
        source.onStatus = { [weak self] in self?.pair.android.setStatus($0) }
        source.onFrame = { [weak self] buffer in
            guard let self, let size = self.pair.android.enqueue(buffer) else { return }
            DispatchQueue.main.async { if size != self.androidSize { self.resized(android: size) } }
        }
        let input = ScrcpyInput(session: session)
        input.onError = { [weak self] in self?.log($0) }
        pair.android.input = input
        androidInput = input
        androidSource = source
        source.start()
    }

    private var hidHelper: URL? {
        let helper = Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("dmb-hid")
        return helper.flatMap { FileManager.default.isExecutableFile(atPath: $0.path) ? $0 : nil }
    }

    /// The iPhone's screen and touch over one Device Hub session, which keeps
    /// the phone's real status bar; the capture path without one
    private func startIPhone() {
        if let replay = options.iphoneReplay {
            run(ReplaySource(urls: [URL(fileURLWithPath: replay)]))
            return
        }
        if let udid = options.udid, let helper = hidHelper {
            let hid = HIDInput(helper: helper, udid: udid, video: true)
            hid.onError = { [weak self] in self?.log($0) }
            hid.onReady = { [weak self] in self?.log("iPhone input ready") }
            hid.onFailed = { [weak self] in self?.captureIPhone() }
            let hub = DeviceHubSource(hid: hid)
            hub.onScreenSize = { [weak self] in self?.pair.iphone.screenSize = $0 }
            pair.iphone.input = hid
            self.hid = hid
            run(hub)
        } else {
            captureIPhone()
        }
    }

    /// Before iOS 27 there is no Device Hub: capture the screen, with touch
    /// through a dmb-hid session started on its first frame
    private func captureIPhone() {
        log("no Device Hub stream for the iPhone, capturing its screen instead")
        pair.iphone.screenSize = nil
        if let udid = options.udid, let helper = hidHelper {
            let hid = HIDInput(helper: helper, udid: udid)
            hid.onError = { [weak self] in self?.log($0) }
            pair.iphone.input = hid
            self.hid = hid
        }
        run(CaptureSource(name: options.name))
    }

    private func run(_ source: FrameSource) {
        source.onStatus = { [weak self] text in DispatchQueue.main.async { self?.pair.iphone.setStatus(text) } }
        source.onFrame = { [weak self] buffer in
            guard let self, let size = self.pair.iphone.enqueue(buffer) else { return }
            DispatchQueue.main.async { if size != self.iphoneSize { self.resized(iphone: size) } }
        }
        iphoneSource = source
        source.start()
    }

    private func resized(android size: CGSize) {
        if androidSize == .zero { pair.android.setStatus(nil) }
        androidSize = size
        pair.androidAspect = size.width / size.height
        fitWindow()
    }

    private func resized(iphone size: CGSize) {
        if iphoneSize == .zero {
            pair.iphone.setStatus(nil)
            // With capture the phone reconnects its USB once it starts, so
            // input waits for it
            if let hid, !hid.video {
                do { try hid.start() } catch { log("cannot start dmb-hid: \(error)") }
            }
        }
        iphoneSize = size
        pair.iphoneAspect = size.width / size.height
        fitWindow()
    }

    /// Keeps the height and the center when a phone turns or first shows
    private func fitWindow() {
        guard !window.styleMask.contains(.fullScreen) else { pair.needsLayout = true; return }
        var frame = window.frame
        let width = pair.width(forHeight: frame.height)
        frame.origin.x += (frame.width - width) / 2
        frame.size.width = width
        window.setFrame(frame, display: true)
        pair.needsLayout = true
        styler.applyStyle()
    }

    /// The phone the pointer last used takes Home, Lock and typing
    private var activeInput: InputSink? {
        (window.firstResponder as? MirrorView)?.input ?? androidInput
    }

    private func buildMenu() {
        let main = NSMenu()
        let appItem = NSMenuItem()
        let app = NSMenu()
        app.addItem(withTitle: "Quit Twin", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = app
        main.addItem(appItem)
        let deviceItem = NSMenuItem()
        let device = NSMenu(title: "Device")
        device.addItem(withTitle: "Home", action: #selector(home), keyEquivalent: "h").target = self
        device.addItem(withTitle: "Lock", action: #selector(lock), keyEquivalent: "l").target = self
        device.addItem(withTitle: "Back", action: #selector(back), keyEquivalent: "b").target = self
        device.addItem(withTitle: "Close", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "w")
        device.addItem(withTitle: "Minimize", action: #selector(NSWindow.miniaturize(_:)), keyEquivalent: "m")
        deviceItem.submenu = device
        main.addItem(deviceItem)
        NSApp.mainMenu = main
    }

    @objc private func home() { activeInput?.button("home") }
    @objc private func lock() { activeInput?.button("lock") }
    @objc private func back() { activeInput?.button("back") }

    public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    public func applicationWillTerminate(_ notification: Notification) {
        androidSource?.stop()
        iphoneSource?.stop()
        hid?.stop()
    }
}

