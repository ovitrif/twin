import AppKit
import AVFoundation
import DMBCore

// Twin's Android window: an Android phone alone, or beside an iPhone, a gap
// apart and the same height. One window is one app to macOS, so window
// managers (Raycast's Center), Mission Control and Cmd-Tab treat the pair as
// one, and it moves as one.
//
//   --serial <serial>        the Android phone (adb)
//   --udid <udid>            the iPhone beside it (none: the Android phone alone)
//   --name <phone name>      the iPhone's capture device name (default: the first iPhone)
//   --product <type>         the iPhone's productType, for its corner radius
//   --height <points>        initial height (default 80% of the screen)
//   --gap <points>           space between the phones (default 12)
//   --android-title <text>   default Android
//   --iphone-title <text>    default iPhone
//   --cursor touch|system    the round touch pointer (default) or macOS's own
//   --server <file>          scrcpy-server 5.0.1 (default: the one in this app)
//   --on-top                 keep the window above other windows
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
    var onTop = false
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
            case "--on-top": onTop = true
            case "--log": log = it.next()
            case "--android-replay": androidReplay = it.next()
            case "--iphone-replay": iphoneReplay = it.next()
            default: break
            }
        }
    }

    public var isEmpty: Bool { serial == nil && androidReplay == nil }

    /// Whether an iPhone shows beside the Android phone
    var hasIPhone: Bool { udid != nil || iphoneReplay != nil }
}

/// Lays out the two phones a gap apart, centered, at one scale: each phone's
/// long side has the same length, so a phone turned sideways keeps its size
/// and lies across instead of growing to the other phone's height.
final class PairView: NSView {
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

    /// A phone's width and height for a long side of 1
    private func shape(_ aspect: CGFloat) -> CGSize {
        aspect > 1 ? CGSize(width: 1, height: 1 / aspect) : CGSize(width: aspect, height: 1)
    }

    private var shown: [CGFloat] {
        (showsAndroid ? [androidAspect] : []) + (showsIPhone ? [iphoneAspect] : [])
    }

    private var widths: CGFloat { shown.map { shape($0).width }.reduce(0, +) }
    private var tallest: CGFloat { shown.map { shape($0).height }.max() ?? 1 }

    /// The window size for this long side
    func size(forLong long: CGFloat) -> CGSize {
        CGSize(width: (long * widths + gaps).rounded(), height: (long * tallest).rounded())
    }

    /// The longest long side that fits this size
    func long(fitting size: CGSize) -> CGFloat {
        guard widths > 0, tallest > 0 else { return 0 }
        return max(0, min(size.height / tallest, (size.width - gaps) / widths).rounded(.down))
    }

    private var gaps: CGFloat { showsAndroid && showsIPhone ? gap : 0 }

    override func layout() {
        super.layout()
        // Full screen leaves room around the pair; fit and center it, each
        // phone centered on the window's height
        let long = long(fitting: bounds.size)
        let a = showsAndroid ? shape(androidAspect) : .zero
        let i = showsIPhone ? shape(iphoneAspect) : .zero
        let aSize = CGSize(width: (a.width * long).rounded(), height: (a.height * long).rounded())
        let iSize = CGSize(width: (i.width * long).rounded(), height: (i.height * long).rounded())
        var x = ((bounds.width - (aSize.width + gaps + iSize.width)) / 2).rounded()
        android.isHidden = !showsAndroid
        iphone.isHidden = !showsIPhone
        android.frame = NSRect(x: x, y: ((bounds.height - aSize.height) / 2).rounded(), width: aSize.width, height: aSize.height)
        x += aSize.width + gaps
        iphone.frame = NSRect(x: x, y: ((bounds.height - iSize.height) / 2).rounded(), width: iSize.width, height: iSize.height)
        let fullScreen = window?.styleMask.contains(.fullScreen) == true
        for pane in [android, iphone] {
            pane.cornerRadius = fullScreen ? 0 : (min(pane.frame.width, pane.frame.height) * cornerRatio).rounded()
        }
    }
}

enum PairSide: Equatable { case android, iphone }

/// Decides which panes a pair window shows from user toggles (`dmb pane`) and
/// live connection state: a disconnected phone drops out of the pair cleanly
/// and returns to its slot automatically when it reconnects.
struct PairPaneState {
    struct Layout: Equatable {
        var showsAndroid: Bool
        var showsIPhone: Bool
        var shouldTerminate: Bool
        var androidStatus: String? = nil
        var iphoneStatus: String? = nil
    }

    let hasIPhone: Bool
    let androidTitle: String
    let iphoneTitle: String
    var userShowsAndroid = true
    var userShowsIPhone: Bool
    var androidConnected = true
    var iphoneConnected: Bool
    private(set) var lastShown: PairSide = .android

    init(hasIPhone: Bool, androidTitle: String = "Android", iphoneTitle: String = "iPhone") {
        self.hasIPhone = hasIPhone
        self.androidTitle = androidTitle
        self.iphoneTitle = iphoneTitle
        self.userShowsIPhone = hasIPhone
        self.iphoneConnected = hasIPhone
    }

    mutating func setConnected(_ side: PairSide, _ connected: Bool) {
        switch side {
        case .android: androidConnected = connected
        case .iphone: iphoneConnected = connected
        }
        if connected {
            lastShown = side
        } else if side == .android && hasIPhone && userShowsIPhone && iphoneConnected {
            lastShown = .iphone
        } else if side == .iphone && userShowsAndroid && androidConnected {
            lastShown = .android
        }
    }

    /// Applies a `dmb pane` hide/show from the menu bar; hiding the only
    /// connected pane closes the window instead of leaving a dead placeholder
    mutating func setUserVisible(_ side: PairSide, _ visible: Bool) -> Layout {
        switch side {
        case .android: userShowsAndroid = visible
        case .iphone: userShowsIPhone = visible
        }
        if !visible {
            let anyLive = (userShowsAndroid && androidConnected) || (hasIPhone && userShowsIPhone && iphoneConnected)
            if !anyLive {
                return Layout(showsAndroid: false, showsIPhone: false, shouldTerminate: true)
            }
        }
        return layout()
    }

    func layout() -> Layout {
        if !userShowsAndroid && (!hasIPhone || !userShowsIPhone) {
            return Layout(showsAndroid: false, showsIPhone: false, shouldTerminate: true)
        }
        let wantAndroid = userShowsAndroid && androidConnected
        let wantIPhone = hasIPhone && userShowsIPhone && iphoneConnected
        if wantAndroid || wantIPhone {
            return Layout(showsAndroid: wantAndroid, showsIPhone: wantIPhone, shouldTerminate: false)
        }
        // Neither wanted phone is connected: keep one cleared dark pane open
        // so the window stays alive and expands back on reconnect
        let showAndroid: Bool
        if hasIPhone && userShowsAndroid && userShowsIPhone {
            showAndroid = lastShown == .android
        } else {
            showAndroid = userShowsAndroid
        }
        let msg: String
        if hasIPhone && userShowsAndroid && userShowsIPhone {
            msg = "Phones disconnected\nWaiting to reconnect…"
        } else if showAndroid {
            msg = "\(androidTitle) disconnected\nWaiting to reconnect…"
        } else {
            msg = "\(iphoneTitle) disconnected\nWaiting to reconnect…"
        }
        return Layout(
            showsAndroid: showAndroid,
            showsIPhone: !showAndroid,
            shouldTerminate: false,
            androidStatus: showAndroid ? msg : nil,
            iphoneStatus: showAndroid ? nil : msg
        )
    }
}

public final class PairDelegate: NSObject, NSApplicationDelegate {
    private let options: PairOptions
    private var window: MirrorWindow!
    private var pair: PairView!
    private var styler: WindowStyler!
    private var paneState: PairPaneState!
    private var androidSource: FrameSource?
    private var iphoneSource: FrameSource?
    private var hid: HIDInput?
    private var androidInput: ScrcpyInput?
    private var parentWatch: DispatchSourceProcess?
    private var androidSize = CGSize.zero
    private var iphoneSize = CGSize.zero
    /// The phones' long side as the user sized the window; turning a phone
    /// keeps it, unless the pair would not fit the screen
    private var chosenLong: CGFloat = 0

    public init(options: PairOptions) { self.options = options }

    private func log(_ text: String) { FileHandle.standardError.write(Data("dmb: \(text)\n".utf8)) }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        if let log = options.log { freopen(log, "a", stderr) }
        parentWatch = watchParent(options.parent)
        buildMenu(target: self, home: #selector(home), lock: #selector(lock), back: #selector(back))

        let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let height = options.height ?? (screen.height * 0.8).rounded()
        paneState = PairPaneState(hasIPhone: options.hasIPhone,
                                  androidTitle: options.androidTitle,
                                  iphoneTitle: options.iphoneTitle)
        pair = PairView(frame: .zero)
        pair.showsIPhone = options.hasIPhone
        pair.gap = options.gap
        pair.cornerRatio = Screen.cornerRatio(productType: options.product)
        let width = pair.size(forLong: height).width
        window = MirrorWindow(contentRect: NSRect(x: screen.midX - width / 2, y: screen.midY - height / 2,
                                                  width: width, height: height),
                              styleMask: [.borderless, .miniaturizable], backing: .buffered, defer: false)
        window.collectionBehavior = [.fullScreenPrimary]
        window.isReleasedWhenClosed = false
        window.acceptsMouseMovedEvents = true
        window.title = options.hasIPhone ? "\(options.androidTitle) and \(options.iphoneTitle)" : options.androidTitle
        if options.onTop { window.level = .floating }
        pair.frame = NSRect(x: 0, y: 0, width: width, height: height)
        pair.autoresizingMask = [.width, .height]
        window.contentView = pair
        chosenLong = height
        NotificationCenter.default.addObserver(forName: NSWindow.didEndLiveResizeNotification, object: window,
                                               queue: .main) { [weak self] _ in
            guard let self else { return }
            self.chosenLong = self.pair.long(fitting: self.window.frame.size)
        }
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
        if options.hasIPhone {
            startIPhone()
            listenForPanes()
        }
    }

    /// `dmb pane` hides or shows one phone of this pair, so the menu bar app can stop or
    /// start one side without closing the other
    private func listenForPanes() {
        DistributedNotificationCenter.default().addObserver(
            forName: PaneRequest.name, object: options.udid ?? options.iphoneReplay, queue: .main
        ) { [weak self] note in
            guard let self, let request = PaneRequest(note.userInfo) else { return }
            let side: PairSide = request.pane == .android ? .android : .iphone
            let layout = self.paneState.setUserVisible(side, request.visible)
            self.applyPaneLayout(layout)
        }
    }

    private func applyPaneLayout(_ layout: PairPaneState.Layout, forceFit: Bool = false) {
        if layout.shouldTerminate {
            NSApp.terminate(nil)
            return
        }
        let changed = pair.showsAndroid != layout.showsAndroid || pair.showsIPhone != layout.showsIPhone
        pair.showsAndroid = layout.showsAndroid
        pair.showsIPhone = layout.showsIPhone
        if let text = layout.androidStatus { pair.android.setStatus(text) }
        if let text = layout.iphoneStatus { pair.iphone.setStatus(text) }
        // Typing and Home go to the phone still shown
        if !pair.showsAndroid, pair.showsIPhone, window.firstResponder === pair.android {
            window.makeFirstResponder(pair.iphone)
        } else if !pair.showsIPhone, pair.showsAndroid, window.firstResponder === pair.iphone {
            window.makeFirstResponder(pair.android)
        }
        if pair.showsAndroid && pair.showsIPhone {
            window.title = "\(options.androidTitle) and \(options.iphoneTitle)"
        } else if pair.showsAndroid {
            window.title = options.androidTitle
        } else if pair.showsIPhone {
            window.title = options.iphoneTitle
        }
        if changed || forceFit { fitWindow() }
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
        let source = ScrcpySource(serial: serial, adb: adb, server: server)
        source.onError = { [weak self] in self?.log($0) }
        source.onStatus = { [weak self] text in DispatchQueue.main.async { self?.pair.android.setStatus(text) } }
        source.onDisconnected = { [weak self] in
            guard let self else { return }
            self.pair.android.clear()
            self.androidSize = .zero
            self.paneState.setConnected(.android, false)
            self.applyPaneLayout(self.paneState.layout())
        }
        source.onFrame = { [weak self] buffer in
            guard let self, let size = self.pair.android.enqueue(buffer) else { return }
            DispatchQueue.main.async {
                if size != self.androidSize || !self.paneState.androidConnected {
                    self.resized(android: size)
                }
            }
        }
        pair.android.onDecodeError = { [weak self] in self?.log("Android video decode failed: \($0)") }
        let input = ScrcpyInput(source: source)
        input.onError = { [weak self] in self?.log($0) }
        pair.android.input = input
        androidInput = input
        androidSource = source
        source.start()
    }

    /// The iPhone's screen and touch over one Device Hub session, which keeps
    /// the phone's real status bar; the capture path without one
    private func startIPhone() {
        if let replay = options.iphoneReplay {
            run(ReplaySource(urls: [URL(fileURLWithPath: replay)]))
            return
        }
        if let udid = options.udid, let helper = hidHelper {
            let (hid, hub) = deviceHub(helper: helper, udid: udid, view: pair.iphone,
                                       log: { [weak self] in self?.log($0) }) { [weak self] in
                self?.captureIPhone()
            }
            hid.onError = { [weak self] in self?.log($0) }
            hid.onReady = { [weak self] in self?.log("iPhone input ready") }
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
        source.onDisconnected = { [weak self] in
            guard let self else { return }
            self.pair.iphone.clear()
            self.iphoneSize = .zero
            self.paneState.setConnected(.iphone, false)
            self.applyPaneLayout(self.paneState.layout())
        }
        source.onFrame = { [weak self] buffer in
            guard let self, let size = self.pair.iphone.enqueue(buffer) else { return }
            DispatchQueue.main.async {
                if size != self.iphoneSize || !self.paneState.iphoneConnected {
                    self.resized(iphone: size)
                }
            }
        }
        iphoneSource = source
        source.start()
    }

    private func resized(android size: CGSize) {
        let reconnected = !paneState.androidConnected
        if androidSize == .zero || reconnected { pair.android.setStatus(nil) }
        if reconnected { paneState.setConnected(.android, true) }
        androidSize = size
        pair.androidAspect = size.width / size.height
        applyPaneLayout(paneState.layout(), forceFit: true)
    }

    private func resized(iphone size: CGSize) {
        let reconnected = !paneState.iphoneConnected
        if iphoneSize == .zero || reconnected {
            pair.iphone.setStatus(nil)
            // With capture the phone reconnects its USB once it starts, so
            // input waits for it
            if iphoneSize == .zero, let hid, !hid.video {
                do { try hid.start() } catch { log("cannot start dmb-hid: \(error)") }
            }
        }
        if reconnected { paneState.setConnected(.iphone, true) }
        iphoneSize = size
        pair.iphoneAspect = size.width / size.height
        applyPaneLayout(paneState.layout(), forceFit: true)
    }

    /// Keeps the phones' size and the window's center when a phone turns or
    /// first shows; a pair too big for the screen shrinks until it fits, and
    /// the window stays on the screen
    private func fitWindow() {
        guard !window.styleMask.contains(.fullScreen) else { pair.needsLayout = true; return }
        var frame = window.frame
        let visible = (window.screen ?? NSScreen.main)?.visibleFrame ?? frame
        var size = pair.size(forLong: chosenLong)
        if size.width > visible.width || size.height > visible.height {
            size = pair.size(forLong: pair.long(fitting: visible.size))
        }
        let (width, height) = (size.width, size.height)
        frame.origin.x += (frame.width - width) / 2
        frame.origin.y += (frame.height - height) / 2
        frame.size = size
        frame.origin.x = min(max(frame.origin.x, visible.minX), visible.maxX - width)
        frame.origin.y = min(max(frame.origin.y, visible.minY), visible.maxY - height)
        window.setFrame(frame, display: true)
        // The style rounds the panes' new frames, so they lay out first
        pair.needsLayout = true
        pair.layoutSubtreeIfNeeded()
        styler.applyStyle()
    }

    /// The phone the pointer last used takes Home, Lock and typing
    private var activeInput: InputSink? {
        if let active = window.firstResponder as? MirrorView, !active.isHidden { return active.input }
        if pair.showsAndroid { return androidInput }
        if pair.showsIPhone { return hid }
        return androidInput
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

